#[cfg(test)]
pub mod test_utils;

mod config;
mod frontmatter;
mod image_file;
mod markdown_file;
mod markdown_files;
mod obsidian_repository;
mod report;
mod validated_config;
mod wikilink;
mod yaml_frontmatter;

pub mod constants;
pub mod utils;

use crate::config::Config;
use crate::constants::DEFAULT_TIMEZONE;
use crate::frontmatter::FrontMatter;
use crate::markdown_file::MarkdownFile;
use crate::obsidian_repository::ObsidianRepository;
use crate::validated_config::ValidatedConfig;
use crate::yaml_frontmatter::YamlFrontMatter;
use std::error::Error;
use std::path::PathBuf;

pub fn process_obsidian_repository(
    config_path: PathBuf,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let expanded_path = utils::expand_tilde(config_path);

    let mut markdown_file = MarkdownFile::new(expanded_path, DEFAULT_TIMEZONE)?;
    let mut config = if let Some(frontmatter) = &markdown_file.frontmatter {
        Config::from_frontmatter(frontmatter.clone())?
    } else {
        return Err("Config file must have frontmatter".into());
    };

    let validated_config = config.validate()?;

    // ANALYSIS PHASE
    let mut obsidian_repository = ObsidianRepository::new(&validated_config)?;

    // REPORTING PHASE
    obsidian_repository.write_reports(&validated_config)?;

    if config.apply_changes == Some(true) {
        obsidian_repository.persist()?;
        reset_apply_changes(&mut markdown_file, &mut config)?;
    }

    Ok(())
}

fn reset_apply_changes(
    markdown_file: &mut MarkdownFile,
    config: &mut Config,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // this whole thing is a bit of a code smell
    // converting from frontmatter to config
    // making sure to update modified date so we can re-use markdown_file persist
    // which in this case doesn't actually matter but does matter for frontmatter...
    config.apply_changes = Some(false);
    let config_yaml = config.to_yaml_str()?;
    let updated_frontmatter = FrontMatter::from_yaml_str(&config_yaml)?;
    markdown_file.frontmatter = Some(updated_frontmatter);

    let operational_timezone = match &config.operational_timezone {
        Some(time_zone) => time_zone.as_str(),
        None => DEFAULT_TIMEZONE,
    };

    markdown_file
        .frontmatter
        .as_mut()
        .unwrap()
        .set_date_modified_now(operational_timezone);
    markdown_file.persist()?;
    Ok(())
}
