use std::collections::BTreeMap;

use crate::model::Section;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewMode {
    #[default]
    Grid,
    List,
}

impl ViewMode {
    pub fn value(self) -> &'static str {
        match self {
            Self::Grid => "grid",
            Self::List => "list",
        }
    }
}

pub const SECTIONS: [Section; 15] = [
    Section::Essays,
    Section::Reference,
    Section::Practice,
    Section::Words,
    Section::Folders,
    Section::Credentials,
    Section::Documents,
    Section::Resume,
    Section::JobHunt,
    Section::Inventory,
    Section::Websites,
    Section::Contacts,
    Section::Games,
    Section::Tools,
    Section::Settings,
];

pub fn key(section: Section) -> &'static str {
    match section {
        Section::Essays => "view.essays",
        Section::Reference => "view.reference",
        Section::Books => "view.books",
        Section::Practice => "view.practice",
        Section::Words => "view.words",
        Section::Folders => "view.folders",
        Section::Credentials => "view.credentials",
        Section::Documents => "view.documents",
        Section::Resume => "view.resume",
        Section::JobHunt => "view.jobhunt",
        Section::Inventory => "view.inventory",
        Section::Websites => "view.websites",
        Section::Contacts => "view.contacts",
        Section::Games => "view.games",
        Section::Tools => "view.tools",
        Section::Settings => "view.settings",
    }
}

pub struct Preferences(BTreeMap<String, ViewMode>);

impl Preferences {
    pub fn load(settings: &BTreeMap<String, String>) -> Self {
        Self(
            SECTIONS
                .into_iter()
                .filter_map(|section| {
                    let mode = match settings.get(key(section)).map(String::as_str) {
                        Some("list") => ViewMode::List,
                        Some("grid") => ViewMode::Grid,
                        _ => return None,
                    };
                    Some((key(section).into(), mode))
                })
                .collect(),
        )
    }

    pub fn mode(&self, section: Section) -> ViewMode {
        self.0
            .get(key(section))
            .copied()
            .unwrap_or(if section == Section::Folders {
                ViewMode::List
            } else {
                ViewMode::Grid
            })
    }

    pub fn set(&mut self, section: Section, mode: ViewMode) {
        self.0.insert(key(section).into(), mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn view_preferences_are_independent_persistent_and_tolerate_unknown_values() {
        let directory = tempfile::tempdir().unwrap();
        let mut db = Database::open(directory.path()).unwrap();
        db.set_setting("unrelated.preference", "keep-me").unwrap();
        let preferences = Preferences::load(&db.settings().unwrap());
        assert_eq!(preferences.mode(Section::Folders), ViewMode::List);
        assert_eq!(preferences.mode(Section::Credentials), ViewMode::Grid);
        db.set_setting(key(Section::Folders), "grid").unwrap();
        db.set_setting(key(Section::Credentials), "list").unwrap();
        db.set_setting(key(Section::Documents), "unknown-future-value")
            .unwrap();
        drop(db);
        let db = Database::open(directory.path()).unwrap();
        let settings = db.settings().unwrap();
        let preferences = Preferences::load(&settings);
        assert_eq!(preferences.mode(Section::Folders), ViewMode::Grid);
        assert_eq!(preferences.mode(Section::Credentials), ViewMode::List);
        assert_eq!(preferences.mode(Section::Documents), ViewMode::Grid);
        assert_eq!(preferences.mode(Section::Contacts), ViewMode::Grid);
        assert_eq!(settings["unrelated.preference"], "keep-me");
    }
}
