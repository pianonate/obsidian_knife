use super::*;
use crate::frontmatter::FrontMatter;
use crate::yaml_frontmatter::YamlFrontMatter;
use std::error::Error;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_persist_frontmatter() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.md");

    // Create initial content
    let initial_content = r#"---
date_created: "2024-01-01"
---
# Test Content"#;
    fs::write(&file_path, initial_content)?;

    let mut file_info = MarkdownFileInfo::new(file_path.clone())?;
    file_info.frontmatter = Some(FrontMatter::from_markdown_str(initial_content)?);

    // Update frontmatter directly
    if let Some(fm) = &mut file_info.frontmatter {
        fm.update_date_created(Some("[[2024-01-02]]".to_string()));
        fm.persist(&file_path)?;
    }

    // Verify frontmatter was updated but content preserved
    let updated_content = fs::read_to_string(&file_path)?;
    assert!(updated_content.contains("[[2024-01-02]]"));
    assert!(updated_content.contains("# Test Content"));

    Ok(())
}

#[test]
fn test_persist_frontmatter_preserves_format() -> Result<(), Box<dyn Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.md");

    let initial_content = r#"---
title: Test Doc
tags:
- tag1
- tag2
date_created: "2024-01-01"
---
# Content"#;
    fs::write(&file_path, initial_content)?;

    let mut file_info = MarkdownFileInfo::new(file_path.clone())?;
    file_info.frontmatter = Some(FrontMatter::from_markdown_str(initial_content)?);

    if let Some(fm) = &mut file_info.frontmatter {
        fm.update_date_created(Some("[[2024-01-02]]".to_string()));
        fm.persist(&file_path)?;
    }

    let updated_content = fs::read_to_string(&file_path)?;
    // Match exact YAML format serde_yaml produces
    assert!(updated_content.contains("tags:\n- tag1\n- tag2"));
    assert!(updated_content.contains("[[2024-01-02]]"));

    Ok(())
}
