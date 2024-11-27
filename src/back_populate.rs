use crate::constants::*;
use crate::markdown_file_info::BackPopulateMatch;
use crate::obsidian_repository_info::ObsidianRepositoryInfo;
use crate::utils::{escape_brackets, escape_pipe};
use crate::utils::{ColumnAlignment, ThreadSafeWriter};
use crate::wikilink_types::ToWikilink;
use crate::ValidatedConfig;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::Path;

#[derive(Debug)]
pub struct AmbiguousMatch {
    pub display_text: String,
    pub targets: Vec<String>,
    pub matches: Vec<BackPopulateMatch>,
}

pub fn write_back_populate_tables(
    config: &ValidatedConfig,
    obsidian_repository_info: &mut ObsidianRepositoryInfo,
    writer: &ThreadSafeWriter,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.writeln(LEVEL1, BACK_POPULATE_COUNT_PREFIX)?;

    if let Some(filter) = config.back_populate_file_filter() {
        writer.writeln(
            "",
            &format!(
                "{} {}\n{}\n",
                BACK_POPULATE_FILE_FILTER_PREFIX,
                filter.to_wikilink(),
                BACK_POPULATE_FILE_FILTER_SUFFIX
            ),
        )?;
    }

    let has_matches = obsidian_repository_info
        .markdown_files
        .iter()
        .any(|file| !file.matches.unambiguous.is_empty());

    if !has_matches {
        return Ok(());
    }

    let ambiguous_matches = identify_and_remove_ambiguous_matches(obsidian_repository_info);

    // Write ambiguous matches first if any exist
    write_ambiguous_matches(writer, &ambiguous_matches)?;

    let unambiguous_matches: Vec<BackPopulateMatch> = obsidian_repository_info
        .markdown_files
        .iter()
        .flat_map(|file| file.matches.unambiguous.clone())
        .collect();

    if !unambiguous_matches.is_empty() {
        write_back_populate_table(
            writer,
            &unambiguous_matches,
            true,
            obsidian_repository_info.wikilinks_sorted.len(),
        )?;

        obsidian_repository_info.apply_back_populate_changes()?;
    }

    Ok(())
}

pub fn identify_and_remove_ambiguous_matches(
    obsidian_repository_info: &mut ObsidianRepositoryInfo,
) -> Vec<AmbiguousMatch> {
    // Create a case-insensitive map of targets to their canonical forms
    let mut target_map: HashMap<String, String> = HashMap::new();
    // CHANGED: Now iterating over wikilinks_sorted directly from obsidian_repository_info
    for wikilink in &obsidian_repository_info.wikilinks_sorted {
        let lower_target = wikilink.target.to_lowercase();
        if !target_map.contains_key(&lower_target)
            || wikilink.target.to_lowercase() == wikilink.target
        {
            target_map.insert(lower_target.clone(), wikilink.target.clone());
        }
    }

    // Create a map of lowercased display_text to normalized targets
    let mut display_text_map: HashMap<String, HashSet<String>> = HashMap::new();
    // CHANGED: Now iterating over wikilinks_sorted directly from obsidian_repository_info
    for wikilink in &obsidian_repository_info.wikilinks_sorted {
        let lower_display_text = wikilink.display_text.to_lowercase();
        let lower_target = wikilink.target.to_lowercase();
        if let Some(canonical_target) = target_map.get(&lower_target) {
            display_text_map
                .entry(lower_display_text.clone())
                .or_default()
                .insert(canonical_target.clone());
        }
    }

    // NEW: Vector to store all ambiguous matches we find
    let mut ambiguous_matches = Vec::new();

    // process each file's matches
    for markdown_file in &mut obsidian_repository_info.markdown_files.iter_mut() {
        // NEW: Create a map to group matches by their lowercased found_text within this file
        let mut matches_by_text: HashMap<String, Vec<BackPopulateMatch>> = HashMap::new();

        // NEW: Drain matches from the file into our temporary map
        let file_matches = std::mem::take(&mut markdown_file.matches.unambiguous);
        for match_info in file_matches {
            let lower_found_text = match_info.found_text.to_lowercase();
            matches_by_text
                .entry(lower_found_text)
                .or_default()
                .push(match_info);
        }

        // Process each group of matches
        for (found_text_lower, text_matches) in matches_by_text {
            if let Some(targets) = display_text_map.get(&found_text_lower) {
                if targets.len() > 1 {
                    // This is an ambiguous match - add to ambiguous_matches
                    ambiguous_matches.push(AmbiguousMatch {
                        display_text: found_text_lower.clone(),
                        targets: targets.iter().cloned().collect(),
                        matches: text_matches.clone(),
                    });
                } else {
                    // Unambiguous matches go back into the markdown_file
                    markdown_file.matches.unambiguous.extend(text_matches);
                }
            } else {
                // Handle unclassified matches (log warning and treat as unambiguous)
                println!(
                    "[WARNING] Found unclassified matches for '{}' in file '{}'",
                    found_text_lower,
                    markdown_file.path.display()
                );
                markdown_file.matches.unambiguous.extend(text_matches);
            }
        }
    }

    // Sort ambiguous matches by display text for consistent output
    ambiguous_matches.sort_by(|a, b| a.display_text.cmp(&b.display_text));

    ambiguous_matches
}

#[derive(Debug, Clone)]
struct ConsolidatedMatch {
    file_path: String,
    line_info: Vec<LineInfo>, // Sorted vector of line information
    replacement: String,
    in_markdown_table: bool,
}

#[derive(Debug, Clone)]
struct LineInfo {
    line_number: usize,
    line_text: String,
    positions: Vec<usize>, // Multiple positions for same line
}

fn consolidate_matches(matches: &[&BackPopulateMatch]) -> Vec<ConsolidatedMatch> {
    // First, group by file path and line number
    let mut line_map: HashMap<(String, usize), LineInfo> = HashMap::new();
    let mut file_info: HashMap<String, (String, bool)> = HashMap::new(); // Tracks replacement and table status per file

    // Group matches by file and line
    for match_info in matches {
        let key = (match_info.relative_path.clone(), match_info.line_number);

        // Update or create line info
        let line_info = line_map.entry(key).or_insert(LineInfo {
            // line_number: match_info.line_number,
            line_number: match_info.line_number + match_info.frontmatter_line_count,
            line_text: match_info.line_text.clone(),
            positions: Vec::new(),
        });
        line_info.positions.push(match_info.position);

        // Track file-level information
        file_info.insert(
            match_info.relative_path.clone(),
            (match_info.replacement.clone(), match_info.in_markdown_table),
        );
    }

    // Convert to consolidated matches, sorting lines within each file
    let mut result = Vec::new();
    for (file_path, (replacement, in_markdown_table)) in file_info {
        let mut file_lines: Vec<LineInfo> = line_map
            .iter()
            .filter(|((path, _), _)| path == &file_path)
            .map(|((_, _), line_info)| line_info.clone())
            .collect();

        // Sort lines by line number
        file_lines.sort_by_key(|line| line.line_number);

        result.push(ConsolidatedMatch {
            file_path,
            line_info: file_lines,
            replacement,
            in_markdown_table,
        });
    }

    // Sort consolidated matches by file path
    result.sort_by(|a, b| {
        let file_a = Path::new(&a.file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let file_b = Path::new(&b.file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        file_a.cmp(file_b)
    });

    result
}

fn write_ambiguous_matches(
    writer: &ThreadSafeWriter,
    ambiguous_matches: &[AmbiguousMatch],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if ambiguous_matches.is_empty() {
        return Ok(());
    }

    writer.writeln(LEVEL2, MATCHES_AMBIGUOUS)?;

    for ambiguous_match in ambiguous_matches {
        writer.writeln(
            LEVEL3,
            &format!(
                "\"{}\" matches {} targets:",
                ambiguous_match.display_text,
                ambiguous_match.targets.len(),
            ),
        )?;

        // Write out all possible targets
        for target in &ambiguous_match.targets {
            writer.writeln(
                "",
                &format!(
                    "- \\[\\[{}|{}]]",
                    target.to_wikilink(),
                    ambiguous_match.display_text
                ),
            )?;
        }

        // Reuse existing table writing code for the matches
        write_back_populate_table(writer, &ambiguous_match.matches, false, 0)?;
    }

    Ok(())
}

fn write_back_populate_table(
    writer: &ThreadSafeWriter,
    matches: &[BackPopulateMatch],
    is_unambiguous_match: bool,
    wikilinks_count: usize,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if is_unambiguous_match {
        writer.writeln(LEVEL2, MATCHES_UNAMBIGUOUS)?;
        writer.writeln(
            "",
            &format!(
                "{} {} {}",
                BACK_POPULATE_COUNT_PREFIX, wikilinks_count, BACK_POPULATE_COUNT_SUFFIX
            ),
        )?;
    }

    // Step 1: Group matches by found_text (case-insensitive) using a HashMap
    let mut matches_by_text: HashMap<String, Vec<&BackPopulateMatch>> = HashMap::new();
    for m in matches {
        let key = m.found_text.to_lowercase();
        matches_by_text.entry(key).or_default().push(m);
    }

    // Step 2: Get display text for each group (use first occurrence's case)
    let mut display_text_map: HashMap<String, String> = HashMap::new();
    for m in matches {
        let key = m.found_text.to_lowercase();
        display_text_map
            .entry(key)
            .or_insert_with(|| m.found_text.clone());
    }

    if is_unambiguous_match {
        // Count unique files across all matches
        let unique_files: HashSet<String> =
            matches.iter().map(|m| m.relative_path.clone()).collect();
        writer.writeln(
            "",
            &format!(
                "{} {}",
                format_back_populate_header(matches.len(), unique_files.len()),
                BACK_POPULATE_TABLE_HEADER_SUFFIX,
            ),
        )?;
    }

    // Headers for the tables
    let headers: Vec<&str> = if is_unambiguous_match {
        vec![
            "file name",
            "line",
            COL_TEXT,
            COL_OCCURRENCES,
            COL_WILL_REPLACE_WITH,
            COL_SOURCE_TEXT,
        ]
    } else {
        vec!["file name", "line", COL_TEXT, COL_OCCURRENCES]
    };

    // Step 3: Collect and sort the keys
    let mut sorted_found_texts: Vec<String> = matches_by_text.keys().cloned().collect();
    sorted_found_texts.sort();

    // Step 4: Iterate over the sorted keys
    for found_text_key in sorted_found_texts {
        let text_matches = &matches_by_text[&found_text_key];
        let display_text = &display_text_map[&found_text_key];
        let total_occurrences = text_matches.len();
        let file_paths: HashSet<String> = text_matches
            .iter()
            .map(|m| m.relative_path.clone())
            .collect();

        let level_string = if is_unambiguous_match { LEVEL3 } else { LEVEL4 };

        writer.writeln(
            level_string,
            &format!(
                "found: \"{}\" ({})",
                display_text,
                pluralize_occurrence_in_files(total_occurrences, file_paths.len())
            ),
        )?;

        // Sort matches by file path and line number
        let mut sorted_matches = text_matches.to_vec();
        sorted_matches.sort_by(|a, b| {
            let file_a = Path::new(&a.relative_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let file_b = Path::new(&b.relative_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            // First compare by file name (case-insensitive)
            let file_cmp = file_a.to_lowercase().cmp(&file_b.to_lowercase());
            if file_cmp != std::cmp::Ordering::Equal {
                return file_cmp;
            }

            // Then by line number within the same file
            a.line_number.cmp(&b.line_number)
        });

        // Consolidate matches
        let consolidated = consolidate_matches(&sorted_matches);

        // Prepare rows
        let mut table_rows = Vec::new();

        for m in consolidated {
            let file_path = Path::new(&m.file_path);
            let file_stem = file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

            // Create a row for each line, maintaining the consolidation of occurrences
            for line_info in m.line_info {
                let highlighted_line = highlight_matches(
                    &line_info.line_text,
                    &line_info.positions,
                    display_text.len(),
                );

                let mut row = vec![
                    file_stem.to_wikilink(),
                    line_info.line_number.to_string(),
                    escape_pipe(&highlighted_line),
                    line_info.positions.len().to_string(),
                ];

                // Only add replacement columns for unambiguous matches
                if is_unambiguous_match {
                    let replacement = if m.in_markdown_table {
                        m.replacement.clone()
                    } else {
                        escape_pipe(&m.replacement)
                    };
                    row.push(replacement.clone());
                    row.push(escape_brackets(&replacement));
                }

                table_rows.push(row);
            }
        }

        // Write the table with appropriate column alignments
        let alignments = if is_unambiguous_match {
            vec![
                ColumnAlignment::Left,
                ColumnAlignment::Right,
                ColumnAlignment::Left,
                ColumnAlignment::Center,
                ColumnAlignment::Left,
                ColumnAlignment::Left,
            ]
        } else {
            vec![
                ColumnAlignment::Left,
                ColumnAlignment::Right,
                ColumnAlignment::Left,
                ColumnAlignment::Center,
            ]
        };

        writer.write_markdown_table(&headers, &table_rows, Some(&alignments))?;
        writer.writeln("", "\n---")?;
    }

    Ok(())
}

// Helper function to highlight all instances of a pattern in text
fn highlight_matches(text: &str, positions: &[usize], match_length: usize) -> String {
    let mut result = String::with_capacity(text.len() * 2);
    let mut last_end = 0;

    // Sort positions to ensure we process them in order
    let mut sorted_positions = positions.to_vec();
    sorted_positions.sort_unstable();

    for &start in sorted_positions.iter() {
        let end = start + match_length;

        // Validate UTF-8 boundaries
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            eprintln!(
                "Invalid UTF-8 boundary detected at position {} or {}",
                start, end
            );
            return text.to_string();
        }

        // Add text before the match
        result.push_str(&text[last_end..start]);

        // Add the highlighted match
        result.push_str("<span style=\"color: red;\">");
        result.push_str(&text[start..end]);
        result.push_str("</span>");

        last_end = end;
    }

    // Add any remaining text after the last match
    result.push_str(&text[last_end..]);
    result
}
