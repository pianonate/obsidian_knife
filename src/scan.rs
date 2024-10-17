use sha2::{Sha256, Digest};

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use crate::{constants::IMAGE_EXTENSIONS,
            validated_config::ValidatedConfig};
use std::path::{Path, PathBuf};
use walkdir::{WalkDir};
use regex::Regex;
use crate::sha256_cache::{CacheFileStatus, Sha256Cache};
use crate::thread_safe_writer::{ColumnAlignment, ThreadSafeWriter};

#[derive(Debug)]
pub struct ImageInfo {
    path: PathBuf,
    hash: String,
    references: Vec<String>,
}

pub fn scan_obsidian_folder(config: ValidatedConfig, writer: &ThreadSafeWriter) -> Result<HashMap<PathBuf, ImageInfo>, Box<dyn Error + Send + Sync>> {
    write_scan_start(&config, writer)?;

    let (markdown_files, image_files, other_files) = collect_files(&config, writer)?;
    let image_counts = count_image_types(&image_files);

    write_file_info(writer, &markdown_files, &image_files, &other_files, &image_counts)?;

    let image_info_map = get_image_info_map(&config, &markdown_files, image_files, writer)?;

    Ok(image_info_map)
}

fn get_image_info_map(
    config: &ValidatedConfig,
    markdown_files: &Vec<PathBuf>,
    image_files: Vec<PathBuf>,
    writer: &ThreadSafeWriter
) -> Result<HashMap<PathBuf, ImageInfo>, Box<dyn Error + Send + Sync>> {

    let cache_file_path = config.obsidian_path().join(crate::constants::CACHE_FOLDER).join("image_cache.json");
    let (mut cache, cache_file_status) = Sha256Cache::new(cache_file_path.clone())?;

    write_cache_file_info(writer, &cache_file_path, cache_file_status)?;

    let initial_cache_count = cache.get_initial_count();

    let mut image_info_map = HashMap::new();
    let image_references = collect_image_references(&markdown_files)?;

    for image_path in image_files {
        let (hash, _) = cache.get_or_update(&image_path)?;

        let image_file_name = image_path.file_name().and_then(OsStr::to_str).unwrap_or_default();
        let references: Vec<String> = image_references.iter()
            .filter(|&ref_path| {
                Path::new(ref_path).file_name().and_then(OsStr::to_str).unwrap_or_default() == image_file_name
            })
            .cloned()
            .collect();

        let image_info = ImageInfo {
            path: image_path.clone(),
            hash,
            references,
        };

        image_info_map.insert(image_path, image_info);
    }

    let files_deleted = cache.remove_non_existent_entries();
    cache.save()?;

    write_cache_contents_info(writer, &mut cache, initial_cache_count, &mut image_info_map, files_deleted)?;

    Ok(image_info_map)
}

fn write_cache_file_info(writer: &ThreadSafeWriter, cache_file_path: &PathBuf, cache_file_status: CacheFileStatus) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln_markdown("##", "collecting image info")?;

    match cache_file_status {
        CacheFileStatus::ReadFromCache => writer.writeln_markdown("", &format!("reading from cache: {:?}", cache_file_path))?,
        CacheFileStatus::CreatedNewCache => writer.writeln_markdown("", &format!("cache file missing - creating new cache: {:?}", cache_file_path))?,
        CacheFileStatus::CacheCorrupted => writer.writeln_markdown("", &format!("cache corrupted, creating new cache: {:?}", cache_file_path))?,
    }
    println!();
    Ok(())
}

fn write_cache_contents_info(writer: &ThreadSafeWriter, cache: &mut Sha256Cache, initial_cache_count: usize, image_info_map: &mut HashMap<PathBuf, ImageInfo>, files_deleted: usize) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (files_read, files_added, files_modified, total_files) = cache.get_stats();

    let headers = &["Metric", "Count"];
    let rows = vec![
        vec!["Total entries in cache (initial)".to_string(), initial_cache_count.to_string()],
        vec!["Matching files read from cache".to_string(), files_read.to_string()],
        vec!["Files added to cache".to_string(), files_added.to_string()],
        vec!["Matching files updated in cache".to_string(), files_modified.to_string()],
        vec!["Files deleted from cache".to_string(), files_deleted.to_string()],
        vec!["Total files in cache (final)".to_string(), total_files.to_string()],
    ];


    let alignments = [ColumnAlignment::Left, ColumnAlignment::Right];
    writer.writeln_markdown("###", "Cache Statistics")?;
    writer.write_markdown_table(headers, &rows, Some(&alignments))?;
    println!();

    assert_eq!(image_info_map.len(), total_files, "The number of entries in image_info_map does not match the total files in cache");
    Ok(())
}

fn hash_file(path: &Path) -> Result<String, Box<dyn Error + Send + Sync>> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 1024];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}


fn collect_files(config: &ValidatedConfig, writer: &ThreadSafeWriter) -> Result<(Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>), Box<dyn Error + Send + Sync>> {
    let ignore_folders = config.ignore_folders().unwrap_or(&[]);

    let mut markdown_files = Vec::new();
    let mut image_files = Vec::new();
    let mut other_files = Vec::new();

    let walker = WalkDir::new(config.obsidian_path()).follow_links(true);

    for entry in walker.into_iter().filter_entry(|e| {
        let is_ignored = ignore_folders.iter().any(|ignored| e.path().starts_with(ignored));
        if is_ignored && e.file_type().is_dir() {
            let _ = writer.writeln_markdown("", &format!("ignoring: {:?}", e.path()));
        }
        !is_ignored
    }) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let path = entry.into_path();
            if path.file_name().and_then(|s| s.to_str()) == Some(".DS_Store") {
                continue;
            }

            match path.extension().and_then(|s| s.to_str()).map(|s| s.to_lowercase()) {
                Some(ext) if ext == "md" => markdown_files.push(path),
                Some(ext) if IMAGE_EXTENSIONS.contains(&ext.as_str()) => image_files.push(path),
                _ => other_files.push(path),
            }
        }
    }

    Ok((markdown_files, image_files, other_files))
}

fn collect_image_references(markdown_files: &[PathBuf]) -> Result<HashSet<String>, Box<dyn Error + Send + Sync>> {
    let extensions_pattern = IMAGE_EXTENSIONS.join("|");
    let image_regex = Regex::new(&format!(
        r"!\[(?:[^\]]*)\]\(([^)]+)\)|!\[\[([^\]]+\.(?:{}))\]\]",
        extensions_pattern
    ))?;
    let mut image_references = HashSet::new();

    for file_path in markdown_files {
        let file = File::open(file_path)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            for capture in image_regex.captures_iter(&line) {
                if let Some(image_name) = capture.get(1).or_else(|| capture.get(2)) {
                    image_references.insert(image_name.as_str().to_string());
                }
            }
        }
    }

    Ok(image_references)
}


fn count_image_types(image_files: &[PathBuf]) -> Vec<(String, usize)> {
    let counts: HashMap<String, usize> = image_files
        .iter()
        .filter_map(|path| path.extension())
        .filter_map(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
        .filter(|ext| IMAGE_EXTENSIONS.contains(&ext.as_str()))
        .fold(HashMap::new(), |mut acc, ext| {
            *acc.entry(ext).or_insert(0) += 1;
            acc
        });

    let mut count_vec: Vec<(String, usize)> = counts.into_iter().collect();
    count_vec.sort_by_key(|&(_, count)| Reverse(count));
    count_vec
}

fn write_file_info(
    output: &ThreadSafeWriter,
    markdown_files: &[PathBuf],
    image_files: &[PathBuf],
    other_files: &[PathBuf],
    image_counts: &[(String, usize)]
) -> Result<(), Box<dyn Error + Send + Sync>> {
    println!();
    output.writeln_markdown("##", "file counts")?;
    output.writeln_markdown("###", &format!("markdown files: {}", markdown_files.len()))?;
    output.writeln_markdown("###", &format!("image files: {}", image_files.len()))?;
    for (ext, count) in image_counts.iter() {
        output.writeln_markdown("- ", &format!(".{}: {}", ext, count))?;
    }
    output.writeln_markdown("###", &format!("other files: {}", other_files.len()))?;

    if !other_files.is_empty() {
        output.writeln_markdown("####", "other files found:")?;
        for file in other_files {
            output.writeln_markdown("- ", &format!("{}", file.display()))?;
        }
    }
    println!();
    Ok(())
}

fn write_scan_start(config: &ValidatedConfig, output: &ThreadSafeWriter) -> Result<(), Box<dyn Error + Send + Sync>> {
    output.writeln_markdown("#", "scanning")?;
    output.writeln_markdown("## scan details", "")?;
    output.writeln_markdown("", &format!("scanning: {:?}", config.obsidian_path()))?;
    Ok(())
}
