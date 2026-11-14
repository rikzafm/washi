//! The fonts folder: `<data folder>\fonts\<id>\regular.ttf` (+ `bold.ttf`, `LICENSE.txt`).
//!
//! Installing writes into `<id>.tmp` next to the final folder and renames it into place, so a
//! crash or a full disk never leaves half a font. Ids are slugs made by [`FontStore::id_for`];
//! anything else is refused before a path is built, so nothing outside the fonts folder is ever
//! written, read or deleted. Which fonts are installed (and their Axis values) lives in the
//! database (`Db::fonts`); this module only owns the files.

use super::{is_valid_font_id, read_package, FontPackage, PackageError};
use std::path::{Path, PathBuf};

const REGULAR: &str = "regular.ttf";
const BOLD: &str = "bold.ttf";
const LICENSE: &str = "LICENSE.txt";

#[derive(Debug)]
pub enum StoreError {
    /// An id that is not a well-formed folder name.
    BadId,
    Io(std::io::Error),
    /// The files on disk are not a usable font any more.
    Broken(PackageError),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::BadId => f.write_str("That is not a font Washi installed."),
            StoreError::Io(e) => write!(f, "Washi could not write the font: {e}"),
            StoreError::Broken(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

/// The fonts folder of one data folder.
#[derive(Clone, Debug)]
pub struct FontStore {
    dir: PathBuf,
}

impl FontStore {
    pub fn new(data_dir: &Path) -> FontStore {
        FontStore { dir: data_dir.join("fonts") }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The id of a family: a slug of its name plus a short hash, so two names that slug alike
    /// still differ, and installing the same family again lands in the same folder.
    pub fn id_for(family: &str) -> String {
        let mut slug = String::new();
        for c in family.trim().to_lowercase().chars() {
            if c.is_ascii_alphanumeric() {
                slug.push(c);
            } else if !slug.ends_with('-') && !slug.is_empty() {
                slug.push('-');
            }
        }
        let slug = slug.trim_end_matches('-');
        let slug: String = slug.chars().take(40).collect();
        let mut h: u32 = 0x811c_9dc5; // FNV-1a
        for b in family.trim().to_lowercase().bytes() {
            h = (h ^ b as u32).wrapping_mul(0x0100_0193);
        }
        if slug.is_empty() {
            format!("font-{h:08x}")
        } else {
            format!("{slug}-{h:08x}")
        }
    }

    fn folder(&self, id: &str) -> Result<PathBuf, StoreError> {
        if is_valid_font_id(id) {
            Ok(self.dir.join(id))
        } else {
            Err(StoreError::BadId)
        }
    }

    /// Whether the font's folder is there.
    pub fn exists(&self, id: &str) -> bool {
        self.folder(id).is_ok_and(|p| p.join(REGULAR).is_file())
    }

    /// Write `pkg` under its id (replacing an earlier install of the family) and return the id.
    pub fn install(&self, pkg: &FontPackage) -> Result<String, StoreError> {
        let id = FontStore::id_for(&pkg.family);
        let target = self.folder(&id)?;
        let tmp = self.dir.join(format!("{id}.tmp"));
        std::fs::create_dir_all(&self.dir)?;
        if tmp.exists() {
            std::fs::remove_dir_all(&tmp)?;
        }
        let written = (|| -> std::io::Result<()> {
            std::fs::create_dir(&tmp)?;
            std::fs::write(tmp.join(REGULAR), &pkg.regular)?;
            if let Some(b) = &pkg.bold {
                std::fs::write(tmp.join(BOLD), b)?;
            }
            if let Some(l) = &pkg.license {
                std::fs::write(tmp.join(LICENSE), l)?;
            }
            Ok(())
        })();
        if let Err(e) = written {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e.into());
        }
        // Move the old install aside, put the new one in place, then drop the old one.
        let old = self.dir.join(format!("{id}.old"));
        if old.exists() {
            let _ = std::fs::remove_dir_all(&old);
        }
        let had_old = target.exists();
        if had_old {
            std::fs::rename(&target, &old).inspect_err(|_| {
                let _ = std::fs::remove_dir_all(&tmp);
            })?;
        }
        if let Err(e) = std::fs::rename(&tmp, &target) {
            if had_old {
                let _ = std::fs::rename(&old, &target);
            }
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e.into());
        }
        if had_old {
            let _ = std::fs::remove_dir_all(&old);
        }
        Ok(id)
    }

    /// Read an installed font back, checking it again as when it was installed.
    pub fn load(&self, id: &str) -> Result<FontPackage, StoreError> {
        let folder = self.folder(id)?;
        let regular = std::fs::read(folder.join(REGULAR))?;
        let mut pkg = read_package(&regular).map_err(StoreError::Broken)?;
        pkg.bold = std::fs::read(folder.join(BOLD)).ok();
        pkg.license = std::fs::read(folder.join(LICENSE)).ok();
        Ok(pkg)
    }

    /// The bytes of an installed font's source file (for the list's preview face).
    pub fn regular_bytes(&self, id: &str) -> Result<Vec<u8>, StoreError> {
        Ok(std::fs::read(self.folder(id)?.join(REGULAR))?)
    }

    /// Delete a font's folder (a missing folder is not an error).
    pub fn remove(&self, id: &str) -> Result<(), StoreError> {
        let folder = self.folder(id)?;
        match std::fs::remove_dir_all(folder) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/snap/tests/fixtures/fonts").join(name)).unwrap()
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    #[test]
    fn installing_writes_the_font_folder_and_leaves_no_temporary_folder_behind() {
        let data = tempfile::tempdir().unwrap();
        let store = FontStore::new(data.path());
        let pkg = read_package(&fixture("Test_Serif.zip")).unwrap();
        let id = store.install(&pkg).unwrap();
        assert!(is_valid_font_id(&id), "{id}");
        assert_eq!(names(data.path()), ["fonts"], "nothing outside the fonts folder");
        assert_eq!(names(store.dir()), [id.clone()], "no .tmp or .old left");
        let files = names(&store.dir().join(&id));
        assert!(files.contains(&"regular.ttf".to_string()) && files.contains(&"bold.ttf".to_string()), "{files:?}");
        assert!(store.exists(&id));
    }

    #[test]
    fn an_installed_font_loads_back_as_the_same_package() {
        let data = tempfile::tempdir().unwrap();
        let store = FontStore::new(data.path());
        let pkg = read_package(&fixture("Test_Sans.zip")).unwrap();
        let id = store.install(&pkg).unwrap();
        let back = store.load(&id).unwrap();
        assert_eq!((back.family, back.axes), (pkg.family, pkg.axes));
        assert_eq!(back.regular, pkg.regular);
        assert_eq!(back.bold, pkg.bold);
    }

    #[test]
    fn installing_a_family_again_replaces_its_folder_in_place() {
        let data = tempfile::tempdir().unwrap();
        let store = FontStore::new(data.path());
        let pkg = read_package(&fixture("Test_Serif.zip")).unwrap();
        let first = store.install(&pkg).unwrap();
        // the second install has no bold file: the old bold.ttf must not survive it
        let plain = FontPackage { bold: None, ..pkg.clone() };
        let second = store.install(&plain).unwrap();
        assert_eq!(first, second);
        assert_eq!(names(store.dir()), [first.clone()]);
        assert!(!names(&store.dir().join(&first)).contains(&"bold.ttf".to_string()));
    }

    #[test]
    fn removing_deletes_the_folder_and_a_missing_one_is_fine() {
        let data = tempfile::tempdir().unwrap();
        let store = FontStore::new(data.path());
        let id = store.install(&read_package(&fixture("Test_Serif.zip")).unwrap()).unwrap();
        store.remove(&id).unwrap();
        assert!(!store.exists(&id));
        assert!(names(store.dir()).is_empty());
        store.remove(&id).unwrap();
    }

    #[test]
    fn ids_that_are_not_slugs_never_become_paths() {
        let data = tempfile::tempdir().unwrap();
        let store = FontStore::new(data.path());
        store.install(&read_package(&fixture("Test_Serif.zip")).unwrap()).unwrap();
        std::fs::write(data.path().join("keep.txt"), b"x").unwrap();
        for bad in ["..", "../keep.txt", "a/b", "a\\b", "", "default", "UPPER"] {
            assert!(matches!(store.remove(bad), Err(StoreError::BadId)), "{bad:?}");
            assert!(matches!(store.load(bad), Err(StoreError::BadId)), "{bad:?}");
            assert!(!store.exists(bad));
        }
        assert!(data.path().join("keep.txt").exists());
        assert_eq!(names(store.dir()).len(), 1);
    }

    #[test]
    fn a_damaged_font_file_is_reported_not_loaded() {
        let data = tempfile::tempdir().unwrap();
        let store = FontStore::new(data.path());
        let id = store.install(&read_package(&fixture("Test_Serif.zip")).unwrap()).unwrap();
        std::fs::write(store.dir().join(&id).join("regular.ttf"), b"not a font").unwrap();
        assert!(matches!(store.load(&id), Err(StoreError::Broken(_))));
    }

    #[test]
    fn ids_follow_the_family_name() {
        assert_eq!(FontStore::id_for("Open Sans"), FontStore::id_for("  open sans "));
        assert_ne!(FontStore::id_for("Open Sans"), FontStore::id_for("Open-Sans!"));
        assert!(FontStore::id_for("Open Sans").starts_with("open-sans-"));
        assert!(is_valid_font_id(&FontStore::id_for("日本語")));
        assert!(is_valid_font_id(&FontStore::id_for(&"x".repeat(500))));
    }
}
