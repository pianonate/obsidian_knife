use crate::constants::*;
use crate::markdown_file::ImageLinkState;
use crate::obsidian_repository::ObsidianRepository;
use crate::report::{ReportDefinition, ReportWriter};
use crate::utils;
use crate::utils::{ColumnAlignment, OutputFileWriter, VecEnumFilter};
use crate::validated_config::ValidatedConfig;
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;

pub struct MissingReferencesTable;

impl ReportDefinition for MissingReferencesTable {
    type Item = (PathBuf, String, usize); // (markdown_path, extracted_filename)

    fn headers(&self) -> Vec<&str> {
        vec![FILE, LINE, MISSING_IMAGE_REFERENCES, ACTION]
    }

    fn alignments(&self) -> Vec<ColumnAlignment> {
        vec![
            ColumnAlignment::Left,
            ColumnAlignment::Right,
            ColumnAlignment::Left,
            ColumnAlignment::Left,
        ]
    }

    fn build_rows(
        &self,
        items: &[Self::Item],
        config: Option<&ValidatedConfig>,
    ) -> Vec<Vec<String>> {
        let mut grouped_references: HashMap<(&PathBuf, usize), Vec<PathBuf>> = HashMap::new();

        for (markdown_path, extracted_filename, line_number) in items {
            grouped_references
                .entry((markdown_path, *line_number))
                .or_default()
                .push(PathBuf::from(extracted_filename));
        }

        let config = config.expect(CONFIG_EXPECT);
        let mut rows: Vec<Vec<String>> = grouped_references
            .iter()
            .map(|((markdown_path, line_number), paths)| {
                let markdown_link =
                    crate::report::format_wikilink(markdown_path, config.obsidian_path(), false);

                // Sort the paths before joining them
                let mut sorted_paths = paths.clone();
                sorted_paths.sort();

                let image_links = sorted_paths
                    .iter()
                    .map(|path| {
                        utils::escape_pipe(&crate::utils::escape_brackets(&path.to_string_lossy()))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");

                let action = if config.apply_changes() {
                    REFERENCE_REMOVED
                } else {
                    REFERENCE_WILL_BE_REMOVED
                };
                vec![
                    markdown_link,
                    line_number.to_string(),
                    image_links,
                    action.to_string(),
                ]
            })
            .collect();

        // Sort rows by markdown link (first column)
        rows.sort_by(|a, b| a[0].cmp(&b[0]));
        rows
    }

    fn title(&self) -> Option<String> {
        Some(MISSING_IMAGE_REFERENCES.to_string())
    }

    fn description(&self, items: &[Self::Item]) -> String {
        DescriptionBuilder::new()
            .pluralize_with_count(Phrase::File(items.len()))
            .pluralize(Phrase::Has(items.len()))
            .text(MISSING_IMAGE)
            .pluralize(Phrase::Reference(items.len()))
            .build()
    }

    fn level(&self) -> &'static str {
        LEVEL2
    }
}

impl ObsidianRepository {
    pub fn write_missing_references_report(
        &self,
        config: &ValidatedConfig,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Collect missing references data in the format the report expects
        let missing_refs: Vec<(PathBuf, String, usize)> = self
            .markdown_files_to_persist
            .iter()
            .flat_map(|file| {
                // Collect missing links into a local variable
                let missing_links = file.image_links.filter_by_variant(ImageLinkState::Missing);
                missing_links.into_iter().map(move |missing| {
                    (
                        file.path.clone(),
                        missing.filename.clone(),
                        missing.line_number,
                    )
                })
            })
            .collect();

        let report = ReportWriter::new(missing_refs).with_validated_config(config);

        report.write(&MissingReferencesTable, writer)
    }
}
