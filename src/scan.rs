#[cfg(test)]
mod process_content_tests;
#[cfg(test)]
mod scan_tests;

use crate::{
    constants::*, markdown_file_info::MarkdownFileInfo,
    obsidian_repository_info::ObsidianRepositoryInfo, wikilink_types::Wikilink,
};

use crate::markdown_files::MarkdownFiles;
use crate::utils::collect_repository_files;
use crate::utils::Sha256Cache;
use crate::utils::Timer;
use crate::wikilink::{create_filename_wikilink, extract_wikilinks};
use crate::wikilink_types::{ExtractedWikilinks, InvalidWikilink};
use crate::ValidatedConfig;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use rayon::prelude::*;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub hash: String,
    pub(crate) references: Vec<String>,
}

pub fn pre_process_obsidian_folder(
    config: &ValidatedConfig,
) -> Result<ObsidianRepositoryInfo, Box<dyn Error + Send + Sync>> {
    let _timer = Timer::new("scan_obsidian_folder");

    let obsidian_repository_info = scan_folders(config)?;

    Ok(obsidian_repository_info)
}

fn get_image_info_map(
    config: &ValidatedConfig,
    markdown_files: &MarkdownFiles,
    image_files: &[PathBuf],
) -> Result<HashMap<PathBuf, ImageInfo>, Box<dyn Error + Send + Sync>> {
    let cache_file_path = config.obsidian_path().join(CACHE_FOLDER).join(CACHE_FILE);
    // Create set of valid paths once
    let valid_paths: HashSet<_> = image_files.iter().map(|p| p.as_path()).collect();

    // let cache = Arc::new(Mutex::new(Sha256Cache::new(cache_file_path.clone())?.0));
    let cache = Arc::new(Mutex::new({
        let mut cache_instance = Sha256Cache::new(cache_file_path.clone())?.0;
        cache_instance.mark_deletions(&valid_paths);
        cache_instance
    }));

    let markdown_refs: HashMap<String, HashSet<String>> = markdown_files
        .par_iter()
        .filter(|file_info| !file_info.image_links.is_empty())
        .map(|file_info| {
            let path = file_info.path.to_string_lossy().to_string();
            let images: HashSet<_> = file_info
                .image_links
                .iter()
                .map(|link| {
                    // Remove ![[]] and any pipe and content after it
                    let clean_name = link
                        .trim_start_matches("![[")
                        .trim_end_matches("]]")
                        .split('|')
                        .next()
                        .unwrap_or("")
                        .to_string();
                    clean_name
                })
                .collect();
            (path, images)
        })
        .collect();

    // Process images
    let image_info_map: HashMap<_, _> = image_files
        .par_iter()
        .filter_map(|image_path| {
            let hash = cache.lock().ok()?.get_or_update(image_path).ok()?.0;

            let image_name = image_path.file_name()?.to_str()?;

            let references: Vec<String> = markdown_refs
                .iter()
                .filter_map(|(path, image_names)| {
                    if image_names.contains(image_name) {
                        Some(path.clone())
                    } else {
                        None
                    }
                })
                .collect();

            Some((image_path.clone(), ImageInfo { hash, references }))
        })
        .collect();

    // Final cache operations
    if let Ok(cache) = Arc::try_unwrap(cache).unwrap().into_inner() {
        if cache.has_changes() {
            cache.save()?;
        }
    }

    Ok(image_info_map)
}

pub fn scan_folders(
    config: &ValidatedConfig,
) -> Result<ObsidianRepositoryInfo, Box<dyn Error + Send + Sync>> {
    let ignore_folders = config.ignore_folders().unwrap_or(&[]);
    let mut obsidian_repository_info = ObsidianRepositoryInfo::default();

    let (markdown_paths, image_files, other_files) =
        collect_repository_files(config, ignore_folders)?;

    obsidian_repository_info.other_files = other_files;

    // Get markdown files info and accumulate all_wikilinks from scan_markdown_files
    let (markdown_files, all_wikilinks) =
        scan_markdown_files(&markdown_paths, config.operational_timezone())?;
    obsidian_repository_info.markdown_files = markdown_files;

    let (sorted, ac) = sort_and_build_wikilinks_ac(all_wikilinks);
    obsidian_repository_info.wikilinks_sorted = sorted;
    obsidian_repository_info.wikilinks_ac = Some(ac);

    // Process image info
    obsidian_repository_info.image_map = get_image_info_map(
        config,
        &obsidian_repository_info.markdown_files,
        &image_files,
    )?;

    Ok(obsidian_repository_info)
}

fn compare_wikilinks(a: &Wikilink, b: &Wikilink) -> std::cmp::Ordering {
    b.display_text
        .len()
        .cmp(&a.display_text.len())
        .then(a.display_text.cmp(&b.display_text))
        .then_with(|| match (a.is_alias, b.is_alias) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.target.cmp(&b.target),
        })
}

fn sort_and_build_wikilinks_ac(all_wikilinks: HashSet<Wikilink>) -> (Vec<Wikilink>, AhoCorasick) {
    let mut wikilinks: Vec<_> = all_wikilinks.into_iter().collect();
    wikilinks.sort_unstable_by(compare_wikilinks);

    let mut patterns = Vec::with_capacity(wikilinks.len());
    patterns.extend(wikilinks.iter().map(|w| w.display_text.as_str()));

    let ac = AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .match_kind(MatchKind::LeftmostLongest)
        .build(&patterns)
        .expect("Failed to build Aho-Corasick automaton for wikilinks");

    (wikilinks, ac)
}

fn scan_markdown_files(
    markdown_paths: &[PathBuf],
    timezone: &str,
) -> Result<(MarkdownFiles, HashSet<Wikilink>), Box<dyn Error + Send + Sync>> {
    let extensions_pattern = IMAGE_EXTENSIONS.join("|");
    let image_regex = Arc::new(Regex::new(&format!(
        r"(!\[(?:[^\]]*)\]\([^)]+\)|!\[\[([^\]]+\.(?:{}))(?:\|[^\]]+)?\]\])",
        extensions_pattern
    ))?);

    // Use Arc<Mutex<...>> for safe shared collection
    let markdown_files = Arc::new(Mutex::new(MarkdownFiles::new()));
    let all_wikilinks = Arc::new(Mutex::new(HashSet::new()));

    markdown_paths.par_iter().try_for_each(|file_path| {
        match scan_markdown_file(file_path, &image_regex, timezone) {
            Ok((file_info, wikilinks)) => {
                markdown_files.lock().unwrap().push(file_info);
                all_wikilinks.lock().unwrap().extend(wikilinks);
                Ok(())
            }
            Err(e) => {
                eprintln!("Error processing file {:?}: {}", file_path, e);
                Err(e)
            }
        }
    })?;

    // Extract data from Arc<Mutex<...>>
    let markdown_info = Arc::try_unwrap(markdown_files)
        .unwrap()
        .into_inner()
        .unwrap();
    let all_wikilinks = Arc::try_unwrap(all_wikilinks)
        .unwrap()
        .into_inner()
        .unwrap();

    Ok((markdown_info, all_wikilinks))
}

fn scan_markdown_file(
    file_path: &PathBuf,
    image_regex: &Arc<Regex>,
    timezone: &str,
) -> Result<(MarkdownFileInfo, Vec<Wikilink>), Box<dyn Error + Send + Sync>> {
    let mut markdown_file_info = MarkdownFileInfo::new(file_path.clone(), timezone)?;

    let aliases = markdown_file_info
        .frontmatter
        .as_ref()
        .and_then(|fm| fm.aliases().cloned());

    // Process content in a single pass
    let (extracted_wikilinks, image_links) = process_content(
        &markdown_file_info.content,
        &aliases,
        file_path,
        image_regex,
    )?;

    // Store results in markdown_file_info
    markdown_file_info.add_invalid_wikilinks(extracted_wikilinks.invalid);
    markdown_file_info.image_links = image_links;

    Ok((markdown_file_info, extracted_wikilinks.valid))
}

fn process_content(
    content: &str,
    aliases: &Option<Vec<String>>,
    file_path: &Path,
    image_regex: &Arc<Regex>,
) -> Result<(ExtractedWikilinks, Vec<String>), Box<dyn Error + Send + Sync>> {
    let mut result = ExtractedWikilinks::default();
    let mut image_links = Vec::new();

    // Add filename-based wikilink
    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    let filename_wikilink = create_filename_wikilink(filename);
    result.valid.push(filename_wikilink.clone());

    // Add aliases if present
    if let Some(alias_list) = aliases {
        for alias in alias_list {
            let wikilink = Wikilink {
                display_text: alias.clone(),
                target: filename_wikilink.target.clone(),
                is_alias: true,
            };
            result.valid.push(wikilink);
        }
    }

    // Process content line by line for both wikilinks and images
    for (line_idx, line) in content.lines().enumerate() {
        // Process wikilinks
        let extracted = extract_wikilinks(line);
        result.valid.extend(extracted.valid);

        let invalid_with_lines: Vec<InvalidWikilink> = extracted
            .invalid
            .into_iter()
            .map(|parsed| parsed.into_invalid_wikilink(line.to_string(), line_idx + 1))
            .collect();
        result.invalid.extend(invalid_with_lines);

        // Process image references in the same pass
        for capture in image_regex.captures_iter(line) {
            if let Some(reference) = capture.get(0) {
                image_links.push(reference.as_str().to_string());
            }
        }
    }

    Ok((result, image_links))
}
