use crate::utils;
use crate::yaml_frontmatter_struct;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};

// when we set date_created_fix to None it won't serialize - cool
// the macro adds support for serializing any fields not explicitly named
yaml_frontmatter_struct! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FrontMatter {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub aliases: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub date_created: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub date_created_fix: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub date_modified: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub do_not_back_populate: Option<Vec<String>>,
        #[serde(skip)]
        pub needs_persist: bool,
        #[serde(skip)]
        pub raw_date_created: Option<DateTime<Utc>>,
        #[serde(skip)]
        pub raw_date_modified: Option<DateTime<Utc>>,
    }
}

impl FrontMatter {
    pub fn aliases(&self) -> Option<&Vec<String>> {
        self.aliases.as_ref()
    }

    pub fn date_created(&self) -> Option<&String> {
        self.date_created.as_ref()
    }

    pub fn date_modified(&self) -> Option<&String> {
        self.date_modified.as_ref()
    }

    pub fn date_created_fix(&self) -> Option<&String> {
        self.date_created_fix.as_ref()
    }

    pub fn remove_date_created_fix(&mut self) {
        // setting it to None will cause it to skip serialization
        self.date_created_fix = None;
    }

    // the raw values are what we will update the actual filesystem with
    // if we're changing the create date it's possible no change will be happening otherwise
    // in this case we still need to update the modify date so make sure we set it if it's
    // not already set
    pub fn set_date_created(&mut self, date: DateTime<Utc>, operational_timezone: &str) {
        let tz: chrono_tz::Tz = operational_timezone.parse().unwrap_or(chrono_tz::UTC);
        let local_date = date.with_timezone(&tz);
        self.raw_date_created = Some(date);
        self.date_created = Some(format!("[[{}]]", local_date.format("%Y-%m-%d")));

        if self.raw_date_modified.is_none() {
            self.set_date_modified_now(operational_timezone);
        }

        self.needs_persist = true;
    }

    // we invoke set_modified_date on any changes to MarkdownFile
    // so that we then will persist it with an updated date_modified to match the file
    // date_modified date and this is also the sentinel for doing the persist operation at the
    // end of processing
    pub fn set_date_modified_now(&mut self, operational_timezone: &str) {
        self.set_date_modified(Utc::now(), operational_timezone);
    }

    // we use this when set_date_modified is missing
    pub fn set_date_modified(&mut self, date: DateTime<Utc>, operational_timezone: &str) {
        let tz: chrono_tz::Tz = operational_timezone.parse().unwrap_or(chrono_tz::UTC);
        let local_date = date.with_timezone(&tz);
        self.raw_date_modified = Some(date);
        self.date_modified = Some(format!("[[{}]]", local_date.format("%Y-%m-%d")));
        self.needs_persist = true;
    }

    pub(crate) fn needs_persist(&self) -> bool {
        self.needs_persist
    }

    pub fn get_do_not_back_populate_regexes(&self) -> Option<Vec<Regex>> {
        // first get do_not_back_populate explicit value
        let mut do_not_populate = self.do_not_back_populate.clone().unwrap_or_default();

        // if there are aliases, add them to that as we don't need text on the page to link to this same page
        if let Some(aliases) = self.aliases() {
            do_not_populate.extend(aliases.iter().cloned());
        }

        // if we have values then return them along with their regexes
        if !do_not_populate.is_empty() {
            utils::build_case_insensitive_word_finder(&Some(do_not_populate))
        } else {
            // we got nothing from valid frontmatter
            None
        }
    }
}
