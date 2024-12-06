use crate::constants::*;
use crate::markdown_file_info::{BackPopulateMatch, MarkdownFileInfo};
use crate::utils::Sha256Cache;
use crate::validated_config::ValidatedConfig;
use crate::wikilink::Wikilink;

use crate::obsidian_repository_info::execute_image_deletions;
use crate::obsidian_repository_info::obsidian_repository_info_types::{
    ImageOperations, ImageReferences,
};
use aho_corasick::AhoCorasick;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
pub struct MarkdownFiles {
    pub(crate) files: Vec<MarkdownFileInfo>,
}

impl Deref for MarkdownFiles {
    type Target = Vec<MarkdownFileInfo>;

    fn deref(&self) -> &Self::Target {
        &self.files
    }
}

impl DerefMut for MarkdownFiles {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.files
    }
}

// Add these implementations after the MarkdownFiles struct definition
impl Index<usize> for MarkdownFiles {
    type Output = MarkdownFileInfo;

    fn index(&self, index: usize) -> &Self::Output {
        &self.files[index]
    }
}

impl IndexMut<usize> for MarkdownFiles {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.files[index]
    }
}

impl MarkdownFiles {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    pub fn push(&mut self, file: MarkdownFileInfo) {
        // Note: now takes &mut self
        self.files.push(file);
    }

    pub fn get_mut(&mut self, path: &Path) -> Option<&mut MarkdownFileInfo> {
        self.iter_mut().find(|file| file.path == path)
    }

    pub fn iter(&self) -> impl Iterator<Item = &MarkdownFileInfo> {
        self.files.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut MarkdownFileInfo> {
        self.files.iter_mut()
    }

    pub fn par_iter(&self) -> impl ParallelIterator<Item = &MarkdownFileInfo> {
        self.files.par_iter()
    }

    pub fn process_files_for_back_populate_matches(
        &mut self,
        config: &ValidatedConfig,
        sorted_wikilinks: Vec<&Wikilink>,
        ac: &AhoCorasick,
    ) {
        self.par_iter_mut().for_each(|markdown_file_info| {
            if !cfg!(test) {
                if let Some(filter) = config.back_populate_file_filter() {
                    if !markdown_file_info.path.ends_with(filter) {
                        return;
                    }
                }
            }

            markdown_file_info.process_file_for_back_populate_matches(
                &sorted_wikilinks,
                config,
                ac,
            );
        });
    }

    pub fn unambiguous_matches(&self) -> Vec<BackPopulateMatch> {
        self.iter()
            .flat_map(|file| file.matches.unambiguous.clone())
            .collect()
    }

    pub fn persist_all(
        &self,
        image_operations: ImageOperations,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        execute_image_deletions(&image_operations)?;

        for file_info in &self.files {
            file_info.persist()?;
        }
        Ok(())
    }

    // map of image files to the markdown files that reference them
    pub(crate) fn get_image_info_map(
        &self,
        config: &ValidatedConfig,
        image_files: &[PathBuf],
    ) -> Result<HashMap<PathBuf, ImageReferences>, Box<dyn Error + Send + Sync>> {
        let cache_file_path = config.obsidian_path().join(CACHE_FOLDER).join(CACHE_FILE);

        // Create set of valid paths once
        let valid_paths: HashSet<_> = image_files.iter().map(|p| p.as_path()).collect();

        let cache = Arc::new(Mutex::new({
            let mut cache_instance = Sha256Cache::load_or_create(cache_file_path.clone())?.0;
            cache_instance.mark_deletions(&valid_paths);
            cache_instance
        }));

        // map of markdown_file_info paths to list of image link file names on that markdown file
        // to_lowercase() for comparisons
        let markdown_refs: HashMap<String, HashSet<String>> = self
            .par_iter()
            .filter(|file_info| !file_info.image_links.found.is_empty())
            .map(|markdown_file_info| {
                let path = markdown_file_info.path.to_string_lossy().to_string();
                let images: HashSet<_> = markdown_file_info
                    .image_links
                    .found
                    .iter()
                    .map(|link| link.filename.to_lowercase())
                    .collect();
                (path, images)
            })
            .collect();

        // Process each image file - for each, find all the markdown_file_info's that have
        // image links that reference that image - using to_lowercase() for comparisons
        let image_info_map: HashMap<_, _> = image_files
            .par_iter()
            .filter_map(|image_path| {
                let hash = cache.lock().ok()?.get_or_update(image_path).ok()?.0;

                let image_name = image_path.file_name()?.to_str()?.to_lowercase();

                let references: Vec<String> = markdown_refs
                    .iter()
                    .filter_map(|(path, image_names)| {
                        if image_names.contains(&image_name) {
                            Some(path.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                Some((
                    image_path.clone(),
                    ImageReferences {
                        hash,
                        markdown_file_references: references,
                    },
                ))
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
}
