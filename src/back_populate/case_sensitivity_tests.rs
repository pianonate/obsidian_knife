use crate::back_populate::back_populate_tests::{
    build_aho_corasick, create_markdown_test_file, create_test_environment,
};
use crate::back_populate::{identify_ambiguous_matches, process_line, BackPopulateMatch};
use crate::markdown_file_info::MarkdownFileInfo;
use crate::wikilink_types::Wikilink;
use std::path::PathBuf;

// Helper struct for test cases
struct TestCase {
    content: &'static str,
    wikilink: Wikilink,
    expected_matches: Vec<(&'static str, &'static str)>,
    description: &'static str,
}

fn get_case_sensitivity_test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            content: "test link TEST LINK Test Link",
            wikilink: Wikilink {
                display_text: "Test Link".to_string(),
                target: "Test Link".to_string(),
                is_alias: false,
            },
            expected_matches: vec![
                ("test link", "[[Test Link|test link]]"),
                ("TEST LINK", "[[Test Link|TEST LINK]]"),
                ("Test Link", "[[Test Link]]"), // Exact match
            ],
            description: "Basic case-insensitive matching",
        },
        TestCase {
            content: "josh likes apples",
            wikilink: Wikilink {
                display_text: "josh".to_string(),
                target: "Joshua Strayhorn".to_string(),
                is_alias: true,
            },
            expected_matches: vec![("josh", "[[Joshua Strayhorn|josh]]")],
            description: "Alias case preservation",
        },
        TestCase {
            content: "karen likes math",
            wikilink: Wikilink {
                display_text: "Karen".to_string(),
                target: "Karen McCoy".to_string(),
                is_alias: true,
            },
            expected_matches: vec![("karen", "[[Karen McCoy|karen]]")],
            description: "Alias case preservation when display case differs from content",
        },
        TestCase {
            content: "| Test Link | Another test link |",
            wikilink: Wikilink {
                display_text: "Test Link".to_string(),
                target: "Test Link".to_string(),
                is_alias: false,
            },
            expected_matches: vec![
                ("Test Link", "[[Test Link]]"), // Exact match
                ("test link", "[[Test Link|test link]]"),
            ],
            description: "Case handling in tables",
        },
    ]
}

pub(crate) fn verify_match(
    actual_match: &BackPopulateMatch,
    expected_text: &str,
    expected_base_replacement: &str,
    case_description: &str,
) {
    assert_eq!(
        actual_match.found_text, expected_text,
        "Wrong matched text for case: {}",
        case_description
    );

    let expected_replacement = if actual_match.in_markdown_table {
        expected_base_replacement.replace('|', r"\|")
    } else {
        expected_base_replacement.to_string()
    };

    assert_eq!(
        actual_match.replacement,
        expected_replacement,
        "Wrong replacement for case: {}\nExpected: {}\nActual: {}\nIn table: {}",
        case_description,
        expected_replacement,
        actual_match.replacement,
        actual_match.in_markdown_table
    );
}

#[test]
fn test_case_insensitive_targets() {
    // Create test wikilinks with case variations
    let wikilinks = vec![
        Wikilink {
            display_text: "Amazon".to_string(),
            target: "Amazon".to_string(),
            is_alias: false,
        },
        Wikilink {
            display_text: "amazon".to_string(),
            target: "amazon".to_string(),
            is_alias: false,
        },
    ];

    let matches = vec![
        BackPopulateMatch {
            full_path: PathBuf::from("test1.md"),
            relative_path: "test1.md".to_string(),
            line_number: 1,
            line_text: "- [[Amazon]]".to_string(),
            found_text: "Amazon".to_string(),
            replacement: "[[Amazon]]".to_string(),
            position: 0,
            in_markdown_table: false,
        },
        BackPopulateMatch {
            full_path: PathBuf::from("test1.md"),
            relative_path: "test1.md".to_string(),
            line_number: 2,
            line_text: "- [[amazon]]".to_string(),
            found_text: "amazon".to_string(),
            replacement: "[[amazon]]".to_string(),
            position: 0,
            in_markdown_table: false,
        },
    ];

    let (ambiguous, unambiguous) = identify_ambiguous_matches(&matches, &wikilinks);

    // Should treat case variations of the same target as the same file
    assert_eq!(
        ambiguous.len(),
        0,
        "Case variations of the same target should not be ambiguous"
    );
    assert_eq!(
        unambiguous.len(),
        2,
        "Both matches should be considered unambiguous"
    );
}

#[test]
fn test_case_sensitivity_behavior() {
    // Initialize test environment without specific wikilinks
    let (temp_dir, config, mut repo_info) = create_test_environment(false, None, None, None);

    for case in get_case_sensitivity_test_cases() {
        let file_path =
            create_markdown_test_file(&temp_dir, "test.md", case.content, &mut repo_info);

        // Create a custom wikilink and build AC automaton directly
        let wikilink = case.wikilink;
        let ac = build_aho_corasick(&[wikilink.clone()]);
        let markdown_info = MarkdownFileInfo::new(file_path.clone()).unwrap();

        let matches =
            process_line(0, case.content, &ac, &[&wikilink], &config, &markdown_info).unwrap();

        assert_eq!(
            matches.len(),
            case.expected_matches.len(),
            "Wrong number of matches for case: {}",
            case.description
        );

        for ((expected_text, expected_base_replacement), actual_match) in
            case.expected_matches.iter().zip(matches.iter())
        {
            verify_match(
                actual_match,
                expected_text,
                expected_base_replacement,
                case.description,
            );
        }
    }
}