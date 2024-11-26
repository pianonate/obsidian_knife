use crate::back_populate::identify_and_remove_ambiguous_matches;
use crate::markdown_file_info::{BackPopulateMatch, MarkdownFileInfo};
use crate::obsidian_repository_info::back_populate_tests::create_test_environment;
use crate::scan::scan_folders;
use crate::test_utils::TestFileBuilder;
use crate::wikilink_types::Wikilink;

#[test]
fn test_identify_ambiguous_matches() {
    let (temp_dir, config, mut repo_info) =
        create_test_environment(false, None, Some(vec![]), None);

    repo_info.wikilinks_sorted = vec![
        Wikilink {
            display_text: "Ed".to_string(),
            target: "Ed Barnes".to_string(),
            is_alias: true,
        },
        Wikilink {
            display_text: "Ed".to_string(),
            target: "Ed Stanfield".to_string(),
            is_alias: true,
        },
        Wikilink {
            display_text: "Unique".to_string(),
            target: "Unique Target".to_string(),
            is_alias: false,
        },
    ];

    TestFileBuilder::new()
        .with_content("Ed wrote this")
        .create(&temp_dir, "test1.md");

    TestFileBuilder::new()
        .with_content("Unique wrote this")
        .create(&temp_dir, "test2.md");

    // Create test markdown files with matches
    let mut test_file = MarkdownFileInfo::new(
        temp_dir.path().join("test1.md"),
        config.operational_timezone(),
    )
    .unwrap();
    test_file.matches = vec![BackPopulateMatch {
        relative_path: "test1.md".to_string(),
        line_number: 1,
        frontmatter_line_count: 0,
        line_text: "Ed wrote this".to_string(),
        found_text: "Ed".to_string(),
        replacement: "[[Ed Barnes|Ed]]".to_string(),
        position: 0,
        in_markdown_table: false,
    }];

    let mut test_file2 = MarkdownFileInfo::new(
        temp_dir.path().join("test2.md"),
        config.operational_timezone(),
    )
    .unwrap();
    test_file2.matches = vec![BackPopulateMatch {
        relative_path: "test2.md".to_string(),
        line_number: 1,
        frontmatter_line_count: 0,
        line_text: "Unique wrote this".to_string(),
        found_text: "Unique".to_string(),
        replacement: "[[Unique Target]]".to_string(),
        position: 0,
        in_markdown_table: false,
    }];

    repo_info.markdown_files.push(test_file2);
    repo_info.markdown_files.push(test_file);

    let ambiguous = identify_and_remove_ambiguous_matches(&mut repo_info);

    // Check ambiguous matches
    assert_eq!(ambiguous.len(), 1, "Should have one ambiguous match group");
    assert_eq!(ambiguous[0].display_text, "ed");
    assert_eq!(ambiguous[0].targets.len(), 2);
    assert!(ambiguous[0].targets.contains(&"Ed Barnes".to_string()));
    assert!(ambiguous[0].targets.contains(&"Ed Stanfield".to_string()));

    // Check that unambiguous match remains in markdown_files
    assert_eq!(
        repo_info.markdown_files[1].matches.len(),
        1,
        "Should have one unambiguous match"
    );
    assert_eq!(repo_info.markdown_files[1].matches[0].found_text, "Unique");
}

#[test]
fn test_truly_ambiguous_targets() {
    let (temp_dir, config, _) = create_test_environment(false, None, Some(vec![]), None);

    // Create the test files using TestFileBuilder
    TestFileBuilder::new()
        .with_content("Amazon is huge")
        .create(&temp_dir, "test1.md");

    TestFileBuilder::new()
        .with_content("# Amazon (company)")
        .with_title("amazon (company)".to_string())
        .with_aliases(vec!["Amazon".to_string()])
        .create(&temp_dir, "Amazon (company).md");

    TestFileBuilder::new()
        .with_content("# Amazon (river)")
        .with_title("amazon (river)".to_string())
        .with_aliases(vec!["Amazon".to_string()])
        .create(&temp_dir, "Amazon (river).md");

    // Let scan_folders find all the files and process them
    let mut repo_info = scan_folders(&config).unwrap();
    repo_info.find_all_back_populate_matches(&config).unwrap();

    // Find test1.md in the repository
    let test_file = repo_info
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("test1.md"))
        .expect("Should find test1.md");

    // Verify initial match exists
    assert_eq!(test_file.matches.len(), 1, "Should have one initial match");

    let ambiguous = identify_and_remove_ambiguous_matches(&mut repo_info);

    assert_eq!(
        ambiguous.len(),
        1,
        "Different targets should be identified as ambiguous"
    );
    assert_eq!(ambiguous[0].targets.len(), 2);

    // Verify the match was moved to ambiguous matches
    let test_file = repo_info
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("test1.md"))
        .expect("Should find test1.md");
    assert!(
        test_file.matches.is_empty(),
        "All matches should be moved to ambiguous"
    );
}

#[test]
fn test_mixed_case_and_truly_ambiguous() {
    let (temp_dir, config, _) = create_test_environment(false, None, Some(vec![]), None);

    // Create test files for case variations
    TestFileBuilder::new()
        .with_content("# AWS")
        .with_title("aws".to_string())
        .create(&temp_dir, "AWS.md");

    TestFileBuilder::new()
        .with_content("# aws")
        .with_title("aws".to_string())
        .create(&temp_dir, "aws.md");

    // Create test files for truly ambiguous targets
    TestFileBuilder::new()
        .with_content("# Amazon (company)")
        .with_title("amazon (company)".to_string())
        .with_aliases(vec!["Amazon".to_string()])
        .create(&temp_dir, "Amazon (company).md");

    TestFileBuilder::new()
        .with_content("# Amazon (river)")
        .with_title("amazon (river)".to_string())
        .with_aliases(vec!["Amazon".to_string()])
        .create(&temp_dir, "Amazon (river).md");

    // Create the test file with both types of matches
    TestFileBuilder::new()
        .with_content(
            r#"AWS and aws are the same
Amazon is ambiguous"#,
        )
        .create(&temp_dir, "test1.md");

    // Let scan_folders find all the files and process them
    let mut repo_info = scan_folders(&config).unwrap();
    repo_info.find_all_back_populate_matches(&config).unwrap();

    // Find test1.md in the repository
    let test_file = repo_info
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("test1.md"))
        .expect("Should find test1.md");

    // We should initially have three matches (both cases of AWS and Amazon)
    assert_eq!(
        test_file.matches.len(),
        3,
        "Should have both AWS cases and Amazon matches initially"
    );

    // Verify we found both cases of AWS and Amazon
    let aws_matches: Vec<_> = test_file
        .matches
        .iter()
        .filter(|m| m.found_text.to_lowercase() == "aws")
        .collect();
    assert_eq!(aws_matches.len(), 2, "Should have both cases of AWS");

    let amazon_matches: Vec<_> = test_file
        .matches
        .iter()
        .filter(|m| m.found_text == "Amazon")
        .collect();
    assert_eq!(amazon_matches.len(), 1, "Should have one Amazon match");

    let ambiguous = identify_and_remove_ambiguous_matches(&mut repo_info);

    assert_eq!(
        ambiguous.len(),
        1,
        "Should only identify truly different targets as ambiguous"
    );

    // Find test1.md again after ambiguous matches were processed
    let test_file = repo_info
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("test1.md"))
        .expect("Should find test1.md");

    assert_eq!(
        test_file.matches.len(),
        2,
        "Both AWS case variations should remain as unambiguous"
    );

    // Verify the remaining matches are both AWS-related
    let aws_matches: Vec<_> = test_file
        .matches
        .iter()
        .filter(|m| m.found_text.to_lowercase() == "aws")
        .collect();
    assert_eq!(
        aws_matches.len(),
        2,
        "Should have both AWS case variations remaining"
    );
}

// This test sets up an **ambiguous alias** (`"Nate"`) mapping to two different targets.
// It ensures that the `identify_ambiguous_matches` function correctly **classifies** both instances of `"Nate"` as **ambiguous**.
//
// Validate that the function can handle **both unambiguous and ambiguous matches simultaneously** without interference.
// prior to this the real world failure was that it would find Karen as an alias but not karen
// even though we have a case-insensitive search
// the problem with the old test is that when there wa sno ambiguous matches - then
// the lower case karen wasn't getting stripped out and the test would pass even though the real world failed
// so in this case we are creating a more realistic test that has a mix of ambiguous and unambiguous
#[test]
fn test_combined_ambiguous_and_unambiguous_matches() {
    let (temp_dir, config, _) = create_test_environment(false, None, Some(vec![]), None);

    // Create the files using TestFileBuilder
    TestFileBuilder::new()
        .with_content(
            r#"# Reference Page
Karen is here
karen is here too
Nate was here and so was Nate"#
                .to_string(),
        )
        .with_title("reference page".to_string())
        .create(&temp_dir, "other.md");

    TestFileBuilder::new()
        .with_content("# Karen McCoy's Page".to_string())
        .with_title("karen mccoy".to_string())
        .with_aliases(vec!["Karen".to_string()])
        .create(&temp_dir, "Karen McCoy.md");

    TestFileBuilder::new()
        .with_content("# Nate McCoy's Page".to_string())
        .with_title("nate mccoy".to_string())
        .with_aliases(vec!["Nate".to_string()])
        .create(&temp_dir, "Nate McCoy.md");

    TestFileBuilder::new()
        .with_content("# Nathan Dye's Page".to_string())
        .with_title("nathan dye".to_string())
        .with_aliases(vec!["Nate".to_string()])
        .create(&temp_dir, "Nathan Dye.md");

    // Let scan_folders find all the files and process them
    let mut repo_info = scan_folders(&config).unwrap();
    repo_info.find_all_back_populate_matches(&config).unwrap();

    // Find other.md in the repository
    let other_file = repo_info
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("other.md"))
        .expect("Should find other.md");

    // Count matches by case-insensitive comparison
    let karen_matches: Vec<_> = other_file
        .matches
        .iter()
        .filter(|m| m.found_text.to_lowercase() == "karen")
        .collect();
    assert_eq!(karen_matches.len(), 2, "Should have both cases of Karen");

    // Verify we have both cases
    assert!(
        karen_matches.iter().any(|m| m.found_text == "Karen"),
        "Should find uppercase Karen"
    );
    assert!(
        karen_matches.iter().any(|m| m.found_text == "karen"),
        "Should find lowercase karen"
    );

    // Get ambiguous matches
    let ambiguous_matches = identify_and_remove_ambiguous_matches(&mut repo_info);

    // Verify ambiguous matches
    let nate_matches = ambiguous_matches
        .iter()
        .find(|am| am.display_text == "nate")
        .expect("Should find Nate as ambiguous");

    assert_eq!(
        nate_matches.matches.len(),
        2,
        "Should find both 'Nate' instances as ambiguous"
    );

    assert_eq!(
        nate_matches.targets.len(),
        2,
        "Should have two possible targets for Nate"
    );
    assert!(
        nate_matches.targets.contains(&"Nate McCoy".to_string())
            && nate_matches.targets.contains(&"Nathan Dye".to_string()),
        "Should have both Nate targets"
    );

    // Verify Karen matches remain unambiguous after processing ambiguous matches
    let other_file = repo_info
        .markdown_files
        .iter()
        .find(|f| f.path.ends_with("other.md"))
        .expect("Should find other.md");

    let karen_matches: Vec<_> = other_file
        .matches
        .iter()
        .filter(|m| m.found_text.to_lowercase() == "karen")
        .collect();
    assert_eq!(
        karen_matches.len(),
        2,
        "Both Karen case variations should remain as unambiguous"
    );
}