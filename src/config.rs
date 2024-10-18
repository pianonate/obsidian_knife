use crate::validated_config::ValidatedConfig;
use serde::Deserialize;
use std::error::Error;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    apply_changes: Option<bool>,
    obsidian_path: String,
    ignore_folders: Option<Vec<String>>,
    cleanup_image_files: Option<bool>,
}

impl Config {
    pub fn validate(self) -> Result<ValidatedConfig, Box<dyn Error + Send + Sync>> {
        let expanded_path = expand_tilde(&self.obsidian_path);
        if !expanded_path.exists() {
            return Err(format!("Path does not exist: {:?}", expanded_path).into());
        }

        let mut ignore_folders = self.validate_ignore_folders(&expanded_path)?;

        // Add the cache folder to ignored_folders
        if let Some(folders) = &mut ignore_folders {
            folders.push(expanded_path.join(crate::constants::CACHE_FOLDER));
        } else {
            ignore_folders = Some(vec![expanded_path.join(crate::constants::CACHE_FOLDER)]);
        }

        Ok(ValidatedConfig::new(
            self.apply_changes.unwrap_or(false),
            expanded_path,
            ignore_folders,
            self.cleanup_image_files.unwrap_or(false),
        ))
    }

    fn validate_ignore_folders(
        &self,
        expanded_path: &PathBuf,
    ) -> Result<Option<Vec<PathBuf>>, Box<dyn Error + Send + Sync>> {
        let ignore_folders = if let Some(folders) = &self.ignore_folders {
            if folders.is_empty() {
                None
            } else {
                let mut validated_folders = Vec::new();
                for (index, folder) in folders.iter().enumerate() {
                    if folder.trim().is_empty() {
                        return Err(format!("ignore_folders: entry at index {} is empty or only contains whitespace", index).into());
                    }
                    let full_path = expanded_path.join(folder);
                    if !full_path.exists() {
                        return Err(format!("Ignore folder does not exist: {:?}", full_path).into());
                    }
                    validated_folders.push(full_path);
                }
                Some(validated_folders)
            }
        } else {
            None
        };
        Ok(ignore_folders)
    }
}

fn expand_tilde<P: AsRef<Path>>(path: P) -> PathBuf {
    let path_str = path.as_ref().to_str().unwrap_or("");
    if path_str.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(&path_str[2..]);
        }
    }
    path.as_ref().to_path_buf()
}
