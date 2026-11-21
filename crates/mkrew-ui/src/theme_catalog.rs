//! The themes the UI can show (ADR 0101): the compiled built-ins in table order, then the
//! valid files of `<config>/themes/` by name, plus one error entry per file that is not
//! loaded. A GPUI global; everything that lists or finds a theme reads it.

use std::path::Path;
use std::time::SystemTime;

use gpui::SharedString;

use crate::theme::{self, Palette, Problem, THEMES, Theme};
use crate::theme_file;

/// The largest theme file read; a theme is a couple of kilobytes, so a bigger `*.toml` is a
/// mistake and must not stall the 500 ms poll.
pub const MAX_FILE_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    BuiltIn,
    /// The file name inside the themes folder.
    File(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CatalogTheme {
    pub name: SharedString,
    pub dark: bool,
    pub origin: Origin,
    /// What "New theme from current" writes for this theme.
    pub palette: Palette,
    /// The contrast rules the theme breaks; empty for every built-in.
    pub problems: Vec<Problem>,
    pub theme: Theme,
}

/// A file (or the folder) that is not loaded, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileError {
    pub file: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeCatalog {
    themes: Vec<CatalogTheme>,
    errors: Vec<FileError>,
}

impl gpui::Global for ThemeCatalog {}

impl ThemeCatalog {
    /// Only the built-in themes.
    pub fn builtin() -> Self {
        let themes = THEMES
            .iter()
            .map(|entry| {
                let theme = entry.build();
                CatalogTheme {
                    name: theme.name.clone(),
                    dark: entry.dark,
                    origin: Origin::BuiltIn,
                    palette: entry
                        .palette()
                        .copied()
                        .unwrap_or_else(|| Palette::from_theme(&theme)),
                    problems: theme::problems(&theme),
                    theme,
                }
            })
            .collect();
        Self {
            themes,
            errors: Vec::new(),
        }
    }

    /// The built-ins and the valid files of `dir` (a missing folder holds none).
    pub fn load(dir: &Path) -> Self {
        Self::from_files(read_folder(dir))
    }

    /// The built-ins and these files, `(file name, its text or why it could not be read)`.
    /// Files are taken in file-name order, so "an earlier file" has a meaning; the loaded
    /// ones are then listed by theme name.
    pub fn from_files(mut files: Vec<(String, Result<String, String>)>) -> Self {
        let mut catalog = Self::builtin();
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let mut loaded: Vec<CatalogTheme> = Vec::new();
        for (file, text) in files {
            let parsed = match text.and_then(|text| theme_file::parse(&text)) {
                Ok(parsed) => parsed,
                Err(message) => {
                    catalog.errors.push(FileError { file, message });
                    continue;
                }
            };
            let taken = (catalog.themes.iter().chain(&loaded))
                .find(|t| t.name.eq_ignore_ascii_case(&parsed.name));
            if let Some(other) = taken {
                let by = match &other.origin {
                    Origin::BuiltIn => "the built-in theme".to_owned(),
                    Origin::File(other_file) => other_file.clone(),
                };
                catalog.errors.push(FileError {
                    file,
                    message: format!("Name taken by {by}"),
                });
                continue;
            }
            let theme = Theme::from_palette(parsed.name.clone(), &parsed.palette);
            loaded.push(CatalogTheme {
                name: parsed.name.into(),
                dark: parsed.palette.dark,
                origin: Origin::File(file),
                palette: parsed.palette,
                problems: theme::problems(&theme),
                theme,
            });
        }
        // Stable: two names that differ only in case keep their file order.
        loaded.sort_by_key(|t| t.name.to_lowercase());
        catalog.themes.extend(loaded);
        catalog
    }

    pub fn themes(&self) -> &[CatalogTheme] {
        &self.themes
    }

    pub fn errors(&self) -> &[FileError] {
        &self.errors
    }

    pub fn find(&self, name: &str) -> Option<&CatalogTheme> {
        self.themes.iter().find(|t| &*t.name == name)
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.themes.iter().position(|t| &*t.name == name)
    }

    /// The themes of one appearance, in catalog order (a gallery group).
    pub fn group(&self, dark: bool) -> impl Iterator<Item = &CatalogTheme> {
        self.themes.iter().filter(move |t| t.dark == dark)
    }

    /// The names a slot's Settings list offers: every built-in (a slot may hold the other
    /// appearance, ADR 0078) and the valid files of the slot's own appearance.
    pub fn slot_names(&self, dark: bool) -> Vec<String> {
        self.themes
            .iter()
            .filter(|t| t.origin == Origin::BuiltIn || t.dark == dark)
            .map(|t| t.name.to_string())
            .collect()
    }
}

fn is_theme_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        && path.is_file()
}

fn read_file(path: &Path) -> Result<String, String> {
    let size = std::fs::metadata(path)
        .map_err(|error| format!("cannot read the file: {error}"))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(format!(
            "the file is larger than {} KB",
            MAX_FILE_BYTES / 1024
        ));
    }
    std::fs::read_to_string(path).map_err(|error| format!("cannot read the file: {error}"))
}

/// The `*.toml` files of `dir` as `(file name, text or error)`. A missing folder holds none;
/// a folder that cannot be read is one entry named after it, so the gallery shows one error
/// card with the path and the OS error.
pub fn read_folder(dir: &Path) -> Vec<(String, Result<String, String>)> {
    let folder_error = |error: std::io::Error| {
        (
            dir.display().to_string(),
            Err(format!("cannot read the folder: {error}")),
        )
    };
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => return vec![folder_error(error)],
    };
    let mut files = Vec::new();
    for entry in entries {
        match entry {
            Err(error) => files.push(folder_error(error)),
            Ok(entry) => {
                let path = entry.path();
                if is_theme_file(&path) {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    files.push((name, read_file(&path)));
                }
            }
        }
    }
    files
}

/// The names, times and sizes of the `*.toml` files of a folder: the poll compares two of
/// them to know the catalog is stale. The size is there because two saves inside one clock
/// tick keep the same time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemesStamp(Vec<(String, Option<SystemTime>, u64)>);

impl ThemesStamp {
    pub fn read(dir: &Path) -> Self {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Self::default();
        };
        let mut files: Vec<(String, Option<SystemTime>, u64)> = entries
            .flatten()
            .filter(|entry| is_theme_file(&entry.path()))
            .map(|entry| {
                let meta = entry.metadata().ok();
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    meta.as_ref().and_then(|m| m.modified().ok()),
                    meta.map_or(0, |m| m.len()),
                )
            })
            .collect();
        files.sort();
        Self(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme_file;

    /// A theme file for a copy of Mkrew Dark (`dark`) or Mkrew Light named `name`.
    fn file(name: &str, dark: bool) -> (String, Result<String, String>) {
        let base = if dark { Theme::dark() } else { Theme::light() };
        (
            format!("{name}.toml"),
            Ok(theme_file::render(name, &Palette::from_theme(&base))),
        )
    }

    fn names(catalog: &ThemeCatalog) -> Vec<&str> {
        catalog.themes().iter().map(|t| &*t.name).collect()
    }

    #[test]
    fn built_ins_come_first_in_table_order() {
        let catalog = ThemeCatalog::builtin();
        let table: Vec<&str> = THEMES.iter().map(|e| e.name).collect();
        assert_eq!(names(&catalog), table);
        assert!(catalog.themes().iter().all(|t| t.origin == Origin::BuiltIn));
        assert!(catalog.themes().iter().all(|t| t.problems.is_empty()));
        assert!(catalog.errors().is_empty());
        assert_eq!(catalog.index_of("Mkrew Dark"), Some(0));
        assert_eq!(catalog.find("Mkrew Light").map(|t| t.dark), Some(false));
        assert!(catalog.find("mkrew dark").is_none(), "exact names only");
    }

    #[test]
    fn file_themes_follow_the_built_ins_sorted_by_name() {
        let catalog = ThemeCatalog::from_files(vec![file("zed", true), file("Aurora", false)]);
        let all = names(&catalog);
        let built_in = THEMES.len();
        assert_eq!(&all[built_in..], ["Aurora", "zed"]);
        assert_eq!(
            catalog.find("Aurora").unwrap().origin,
            Origin::File("Aurora.toml".into())
        );
        assert!(catalog.errors().is_empty());
    }

    #[test]
    fn a_name_taken_by_a_built_in_or_an_earlier_file_is_an_error_card() {
        let same = (
            "b.toml".to_owned(),
            Ok(theme_file::render(
                "a",
                &Palette::from_theme(&Theme::dark()),
            )),
        );
        let catalog = ThemeCatalog::from_files(vec![file("Dracula", true), file("a", true), same]);
        let taken: Vec<_> = catalog.errors().iter().map(|e| e.file.as_str()).collect();
        assert_eq!(taken, ["Dracula.toml", "b.toml"]);
        assert!(
            catalog.errors()[0].message.contains("built-in"),
            "{:?}",
            catalog.errors()
        );
        assert!(
            catalog.errors()[1].message.contains("a.toml"),
            "{:?}",
            catalog.errors()
        );
        assert_eq!(names(&catalog).iter().filter(|n| **n == "a").count(), 1);
        assert_eq!(
            names(&catalog).iter().filter(|n| **n == "Dracula").count(),
            1
        );
    }

    #[test]
    fn a_name_differing_only_in_case_is_a_clash() {
        // File-name order is byte order: AURORA.toml is taken before Aurora.toml.
        let catalog = ThemeCatalog::from_files(vec![
            file("dracula", true),
            file("Aurora", true),
            file("AURORA", true),
        ]);
        assert!(catalog.find("dracula").is_none());
        assert!(catalog.find("Dracula").is_some());
        assert!(catalog.find("AURORA").is_some() && catalog.find("Aurora").is_none());
        let taken: Vec<_> = catalog.errors().iter().map(|e| e.file.as_str()).collect();
        assert_eq!(taken, ["Aurora.toml", "dracula.toml"]);
        assert!(
            catalog.errors()[0].message.contains("AURORA.toml"),
            "{:?}",
            catalog.errors()
        );
        assert!(
            catalog.errors()[1].message.contains("built-in"),
            "{:?}",
            catalog.errors()
        );
    }

    #[test]
    fn a_broken_or_unreadable_file_is_an_error_card_and_the_rest_loads() {
        let catalog = ThemeCatalog::from_files(vec![
            ("bad.toml".into(), Ok("oops".into())),
            (
                "gone.toml".into(),
                Err("cannot read the file: denied".into()),
            ),
            file("Good", true),
        ]);
        assert!(catalog.find("Good").is_some());
        let files: Vec<_> = catalog.errors().iter().map(|e| e.file.as_str()).collect();
        assert_eq!(files, ["bad.toml", "gone.toml"]);
        assert_eq!(catalog.errors()[1].message, "cannot read the file: denied");
    }

    #[test]
    fn a_removed_file_leaves_the_catalog() {
        let with = ThemeCatalog::from_files(vec![file("Aurora", true)]);
        assert!(with.find("Aurora").is_some());
        assert_eq!(
            ThemeCatalog::from_files(Vec::new()),
            ThemeCatalog::builtin()
        );
    }

    #[test]
    fn a_low_contrast_file_loads_with_its_problems() {
        let mut palette = Palette::from_theme(&Theme::dark());
        palette.fg = palette.bg;
        let text = theme_file::render("Dim", &palette);
        let catalog = ThemeCatalog::from_files(vec![("dim.toml".into(), Ok(text))]);
        let dim = catalog
            .find("Dim")
            .expect("loaded, the user's file is the user's call");
        assert!(!dim.problems.is_empty());
        assert!(catalog.errors().is_empty());
    }

    #[test]
    fn the_slot_lists_hold_every_built_in_and_the_files_of_their_appearance() {
        let catalog = ThemeCatalog::from_files(vec![file("Aurora", true), file("Dawn", false)]);
        let dark = catalog.slot_names(true);
        let light = catalog.slot_names(false);
        for list in [&dark, &light] {
            assert!(
                list.contains(&"Dracula".to_owned()) && list.contains(&"Mkrew Light".to_owned())
            );
        }
        assert!(dark.contains(&"Aurora".to_owned()) && !dark.contains(&"Dawn".to_owned()));
        assert!(light.contains(&"Dawn".to_owned()) && !light.contains(&"Aurora".to_owned()));
        let groups: Vec<_> = catalog.group(true).map(|t| t.dark).collect();
        assert!(groups.iter().all(|dark| *dark));
        assert_eq!(
            catalog.group(true).count() + catalog.group(false).count(),
            catalog.themes().len()
        );
    }

    #[test]
    fn the_folder_is_read_for_toml_files_only_and_odd_ones_become_errors() {
        let dir = tempfile::tempdir().unwrap();
        let moon = theme_file::render("Moon", &Palette::from_theme(&Theme::dark()));
        std::fs::write(dir.path().join("moon.TOML"), moon).unwrap();
        std::fs::write(dir.path().join("notes.txt"), "x").unwrap();
        std::fs::create_dir(dir.path().join("sub.toml")).unwrap();
        std::fs::write(dir.path().join("binary.toml"), [0xFF, 0xFE, 0x00]).unwrap();
        std::fs::write(
            dir.path().join("huge.toml"),
            vec![b'#'; MAX_FILE_BYTES as usize + 1],
        )
        .unwrap();
        let catalog = ThemeCatalog::load(dir.path());
        assert!(
            catalog.find("Moon").is_some(),
            "the extension is case-insensitive"
        );
        let bad: Vec<_> = catalog.errors().iter().map(|e| e.file.as_str()).collect();
        assert_eq!(bad, ["binary.toml", "huge.toml"]);
        assert!(
            catalog.errors()[1].message.contains("64 KB"),
            "{:?}",
            catalog.errors()
        );
    }

    #[test]
    fn a_missing_folder_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = ThemeCatalog::load(&dir.path().join("themes"));
        assert_eq!(catalog, ThemeCatalog::builtin());
    }

    #[test]
    fn the_stamp_changes_with_every_edit_add_and_removal() {
        let dir = tempfile::tempdir().unwrap();
        let empty = ThemesStamp::read(dir.path());
        assert_eq!(empty, ThemesStamp::read(&dir.path().join("missing")));
        let file = dir.path().join("a.toml");
        std::fs::write(&file, "x").unwrap();
        let one = ThemesStamp::read(dir.path());
        assert_ne!(one, empty);
        // The length differs even when the clock did not tick.
        std::fs::write(&file, "xy").unwrap();
        let two = ThemesStamp::read(dir.path());
        assert_ne!(two, one);
        std::fs::write(dir.path().join("notes.txt"), "z").unwrap();
        assert_eq!(ThemesStamp::read(dir.path()), two, "only .toml files count");
        std::fs::remove_file(&file).unwrap();
        assert_eq!(ThemesStamp::read(dir.path()), empty);
    }

    #[test]
    fn the_catalog_follows_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("moon.toml");
        let mut palette = Palette::from_theme(&Theme::dark());
        std::fs::write(&path, theme_file::render("Moon", &palette)).unwrap();
        let stamp = ThemesStamp::read(dir.path());
        let first = ThemeCatalog::load(dir.path());
        assert_eq!(first.find("Moon").unwrap().theme.bg, palette.bg);

        palette.bg = 0x101820;
        std::fs::write(&path, theme_file::render("Moon", &palette)).unwrap();
        assert_ne!(
            ThemesStamp::read(dir.path()),
            stamp,
            "the poll sees the edit"
        );
        let second = ThemeCatalog::load(dir.path());
        assert_eq!(second.find("Moon").unwrap().theme.bg, 0x101820);
        assert_ne!(first, second);

        std::fs::remove_file(&path).unwrap();
        assert!(ThemeCatalog::load(dir.path()).find("Moon").is_none());
    }

    #[test]
    fn a_written_copy_of_every_theme_loads_back_to_the_same_palette() {
        let original = ThemeCatalog::builtin();
        for listed in original.themes() {
            let name = theme_file::copy_name(&listed.name, |taken| original.find(taken).is_some());
            let text = theme_file::render(&name, &listed.palette);
            let copy = ThemeCatalog::from_files(vec![(theme_file::file_name(&name), Ok(text))]);
            let loaded = copy
                .find(&name)
                .unwrap_or_else(|| panic!("{name} did not load"));
            assert_eq!(loaded.palette, listed.palette, "{name}");
            assert_eq!(loaded.dark, listed.dark, "{name}");
            assert!(copy.errors().is_empty(), "{name}: {:?}", copy.errors());
        }
    }
}
