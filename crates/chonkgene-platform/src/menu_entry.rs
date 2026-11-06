//! The app-menu entry on Linux (PLAN §1.9, "Program icon"): 肉肉賴桌 with
//! its icon among the desktop's applications. A portable program adds one
//! only when asked (`chonkgene --menu-entry`), and `repair` points one that
//! is there at the folder it was moved to. Windows has nothing to add: the
//! exe carries its own icon, and pinning it to Start is up to you.

use std::path::Path;

/// Add the entry (or rewrite it), starting `exe`, with `icon` (a 256 px
/// PNG) as its icon.
pub fn add(exe: &Path, icon: &[u8]) -> Result<(), String> {
    imp::add(exe, icon)
}

/// Take the entry and its icon away again.
pub fn remove() -> Result<(), String> {
    imp::remove()
}

/// If there is an entry, make it start `exe` (the folder may have moved).
pub fn repair(exe: &Path, icon: &[u8]) -> Result<(), String> {
    match imp::current() {
        Some(path) if path != exe => imp::add(exe, icon),
        _ => Ok(()),
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::fs;
    use std::path::{Path, PathBuf};

    use crate::autostart::imp::{exec_in, quote};

    fn data_home() -> Option<PathBuf> {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    }

    fn entry() -> Option<PathBuf> {
        Some(data_home()?.join("applications/chonkgene.desktop"))
    }

    fn icon() -> Option<PathBuf> {
        Some(data_home()?.join("icons/hicolor/256x256/apps/chonkgene.png"))
    }

    pub fn current() -> Option<PathBuf> {
        exec_in(&fs::read_to_string(entry()?).ok()?)
    }

    fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn add(exe: &Path, png: &[u8]) -> Result<(), String> {
        let (entry, icon) = (entry(), icon());
        let (Some(entry), Some(icon)) = (entry, icon) else {
            return Err("no data directory (HOME is not set)".into());
        };
        write(&icon, png)?;
        let text = format!(
            "[Desktop Entry]\nType=Application\nName=肉肉賴桌\nName[zh_TW]=肉肉賴桌\nGenericName=Desktop pets\nComment=Chubby desktop pets\nExec={}\nIcon=chonkgene\nTerminal=false\nCategories=Game;Amusement;\nStartupNotify=false\n",
            quote(exe)
        );
        write(&entry, text.as_bytes())
    }

    pub fn remove() -> Result<(), String> {
        for path in [entry(), icon()].into_iter().flatten() {
            if path.exists() {
                fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// In a folder of its own (XDG_DATA_HOME): added, found, repaired
        /// after a move, and gone again.
        #[test]
        fn an_entry_is_added_repaired_and_removed() {
            let dir = std::env::temp_dir().join(format!("chonkgene-menu-{}", std::process::id()));
            // SAFETY: this test is the only one in the crate touching the
            // variable.
            unsafe { std::env::set_var("XDG_DATA_HOME", &dir) };
            let (a, b) = (
                Path::new("/opt/pets/chonkgene"),
                Path::new("/home/me/pets/chonkgene"),
            );
            add(a, b"png").unwrap();
            assert_eq!(current().as_deref(), Some(a));
            assert!(icon().unwrap().exists());
            super::super::repair(b, b"png").unwrap();
            assert_eq!(current().as_deref(), Some(b));
            remove().unwrap();
            assert_eq!(current(), None);
            assert!(!icon().unwrap().exists());
            let _ = fs::remove_dir_all(&dir);
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use std::path::{Path, PathBuf};

    pub fn current() -> Option<PathBuf> {
        None
    }

    pub fn add(_exe: &Path, _icon: &[u8]) -> Result<(), String> {
        Err("an app-menu entry is for Linux; on Windows pin chonkGene.exe to Start".into())
    }

    pub fn remove() -> Result<(), String> {
        Ok(())
    }
}
