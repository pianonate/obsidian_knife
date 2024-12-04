use crate::utils::{ColumnAlignment, OutputFileWriter};
use crate::validated_config::ValidatedConfig;
use std::error::Error;
use std::path::{Path, PathBuf};

/// definition of the elements of a report to write out as a markdown table
pub trait ReportDefinition<C = ()> {
    /// The type of data being displayed in the table
    type Item;

    /// Get the table headers
    fn headers(&self) -> Vec<&str>;

    /// Get column alignments for the table
    fn alignments(&self) -> Vec<ColumnAlignment>;

    /// Transform data items into table rows
    ///
    /// simple reports can use "_: &()" for this generic parameter so they don't
    /// need to use it and the compiler won't complain
    ///
    /// reports that need config information can use "report_context: &ReportContext"
    /// to access properties such as appLy_changes or obsidian_path
    ///
    /// it's slightly hacky but prevents having to dramatically alter the structure and it's
    /// readable enough
    fn build_rows(&self, items: &[Self::Item], context: &C) -> Vec<Vec<String>>;

    /// Optional table title
    fn title(&self) -> Option<&str> {
        None
    }

    /// Optional table description/summary
    fn description(&self, items: &[Self::Item]) -> String;

    /// markdown level
    fn level(&self) -> &'static str;

    fn hide_title_if_no_rows(&self) -> bool {
        true
    }
}

// using this to get owned values from a ValidatedConfig to make available to
// reports without having to have all kinds of lifetime attributes set
// in ReportWriter and ReportDefinition
#[derive(Clone)]
pub struct ReportContext {
    obsidian_path: PathBuf, // Owned PathBuf instead of borrowed Path
    apply_changes: bool,
    // Add other needed config values here
}

impl ReportContext {
    pub fn new(config: &ValidatedConfig) -> Self {
        Self {
            obsidian_path: config.obsidian_path().to_path_buf(),
            apply_changes: config.apply_changes(),
        }
    }

    pub fn obsidian_path(&self) -> &Path {
        &self.obsidian_path
    }

    pub fn apply_changes(&self) -> bool {
        self.apply_changes
    }
}

/// writes out the TableDefinition
/// the idea is you collect all the items that will get turned into rows and pass them
/// in to the generic Vec<T> parameter
/// then the ReportWriter will call build_rows with the items and the context (if provided)
/// where the definition will do the work to transform items into rows
pub struct ReportWriter<T, C = ()> {
    items: Vec<T>,
    context: C,
}

impl<T> ReportWriter<T, ()> {
    pub fn new(items: Vec<T>) -> Self {
        Self { items, context: () }
    }
    /// Write the table using the provided builder and writer
    pub fn write<B: ReportDefinition<Item = T>>(
        &self,
        report: &B,
        writer: &OutputFileWriter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if self.items.is_empty() && report.hide_title_if_no_rows() {
            return Ok(());
        }

        // Write title if present
        if let Some(title) = report.title() {
            writer.writeln(report.level(), title)?;
        }

        // Write description if present
        writer.writeln("", &report.description(&self.items))?;

        // Skip empty tables unless overridden
        if self.items.is_empty() {
            return Ok(());
        }

        // Build and write the table
        let headers = report.headers();
        let alignments = report.alignments();
        let rows = report.build_rows(&self.items, &self.context);

        writer.write_markdown_table(&headers, &rows, Some(&alignments))?;

        Ok(())
    }
}
