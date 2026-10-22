//! 「開機時一起醒來」 (PLAN §2.3): a user-level login entry, never a system
//! one. `sync` also repairs the entry when the folder has been moved.

use std::path::{Path, PathBuf};

/// Where the login entry points now, if there is one.
pub fn current() -> Option<PathBuf> {
    imp::current()
}

/// Make the login entry match `enabled`, pointing at `exe`.
pub fn sync(enabled: bool, exe: &Path) -> Result<(), String> {
    match (enabled, current()) {
        (true, Some(path)) if path == exe => Ok(()),
        (true, _) => imp::set(exe),
        (false, Some(_)) => imp::remove(),
        (false, None) => Ok(()),
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::fs;
    use std::path::{Path, PathBuf};

    fn entry() -> Option<PathBuf> {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(config.join("autostart").join("chonkgene.desktop"))
    }

    /// Quote a path for a desktop entry's Exec key.
    fn quote(path: &Path) -> String {
        let s = path.to_string_lossy();
        let escaped: String = s
            .chars()
            .flat_map(|c| match c {
                '"' | '`' | '$' | '\\' => vec!['\\', c],
                _ => vec![c],
            })
            .collect();
        format!("\"{escaped}\"")
    }

    pub fn current() -> Option<PathBuf> {
        let text = fs::read_to_string(entry()?).ok()?;
        let exec = text.lines().find_map(|l| l.strip_prefix("Exec="))?;
        let inner = exec.trim().strip_prefix('"')?.strip_suffix('"')?;
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                out.extend(chars.next());
            } else {
                out.push(c);
            }
        }
        Some(PathBuf::from(out))
    }

    pub fn set(exe: &Path) -> Result<(), String> {
        let path = entry().ok_or("no config directory")?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = format!(
            "[Desktop Entry]\nType=Application\nName=肉肉賴桌\nComment=Chubby desktop pets\nExec={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            quote(exe)
        );
        fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn remove() -> Result<(), String> {
        match entry() {
            Some(path) if path.exists() => fs::remove_file(&path).map_err(|e| e.to_string()),
            _ => Ok(()),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn exec_paths_survive_quoting() {
            let path = Path::new("/home/me/My \"Pets\"/chonkgene");
            let quoted = quote(path);
            let inner = quoted.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
            assert!(inner.contains("\\\""));
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
    };

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const NAME: &str = "chonkGene";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    pub fn current() -> Option<PathBuf> {
        let (key, name) = (wide(RUN), wide(NAME));
        let mut buf = vec![0u16; 1024];
        let mut bytes = (buf.len() * 2) as u32;
        // SAFETY: both strings are NUL-terminated; `buf` holds `bytes` bytes.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                null_mut(),
                buf.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let len = (bytes as usize / 2).saturating_sub(1);
        let value = OsString::from_wide(&buf[..len])
            .to_string_lossy()
            .into_owned();
        Some(PathBuf::from(value.trim_matches('"')))
    }

    pub fn set(exe: &Path) -> Result<(), String> {
        let (key, name) = (wide(RUN), wide(NAME));
        let quoted: Vec<u16> = "\""
            .encode_utf16()
            .chain(exe.as_os_str().encode_wide())
            .chain("\"".encode_utf16())
            .chain([0])
            .collect();
        // SAFETY: NUL-terminated strings; the data pointer covers `len` bytes.
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                quoted.as_ptr().cast(),
                (quoted.len() * 2) as u32,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("cannot write the login entry (error {status})"))
        }
    }

    pub fn remove() -> Result<(), String> {
        let (key, name) = (wide(RUN), wide(NAME));
        // SAFETY: both strings are NUL-terminated.
        let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("cannot remove the login entry (error {status})"))
        }
    }
}
