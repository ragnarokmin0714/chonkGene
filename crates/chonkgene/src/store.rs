//! Settings and saves (PLAN §2.2, §3.4): RON files in the portable folder
//! next to the executable, or the user's config directory.
//!
//! Writes go to a temporary file that is then renamed over the old one, so a
//! power cut cannot leave half a file. Reads are size-capped; a file that
//! does not parse is kept as `<name>.bad` and defaults are used.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chonkgene_core::Personality;
use chonkgene_core::mind::Affinity;
use chonkgene_core::species::Species;
use serde::{Deserialize, Serialize};

pub const PORTABLE_DIR: &str = "chonkGene-data";
const MAX_FILE: u64 = 1024 * 1024;
const FORMAT: u32 = 1;

/// Where data lives and whether that is the portable folder.
#[derive(Clone, Debug)]
pub struct DataDir {
    pub path: PathBuf,
    pub portable: bool,
}

impl DataDir {
    /// `chonkGene-data/` next to the executable if it exists, else the
    /// user's config directory (PLAN §2.2).
    pub fn locate(override_dir: Option<PathBuf>) -> io::Result<Self> {
        if let Some(path) = override_dir {
            fs::create_dir_all(&path)?;
            return Ok(Self {
                path,
                portable: true,
            });
        }
        if let Some(dir) = std::env::current_exe()?
            .parent()
            .map(|p| p.join(PORTABLE_DIR))
            && dir.is_dir()
        {
            return Ok(Self {
                path: dir,
                portable: true,
            });
        }
        let path = config_dir().ok_or_else(|| io::Error::other("no config directory"))?;
        fs::create_dir_all(&path)?;
        Ok(Self {
            path,
            portable: false,
        })
    }

    pub fn settings(&self) -> PathBuf {
        self.path.join("settings.ron")
    }

    pub fn pets(&self) -> PathBuf {
        self.path.join("pets.ron")
    }
}

fn config_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("chonkGene"))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|c| c.join("chonkgene"))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub format: u32,
    pub sound: bool,
    /// 0–100.
    pub volume: u8,
    pub irrational: bool,
    /// 0 偶爾, 1 常常, 2 超常.
    pub squabble: u8,
    pub hide_in_fullscreen: bool,
    pub start_at_login: bool,
    /// How many pets are on the desktop at once; no upper limit (PLAN §1.1).
    pub on_desktop: u32,
    /// Set once the first run has been handled.
    pub welcomed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            format: FORMAT,
            sound: true,
            volume: 70,
            irrational: true,
            squabble: 0,
            hide_in_fullscreen: true,
            start_at_login: false,
            on_desktop: 6,
            welcomed: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PetSave {
    pub id: u64,
    pub species: String,
    pub name: String,
    pub you: String,
    pub coat: String,
    /// Body value 0.0 標準 … 1.0 麻糬.
    pub chonk: f32,
    /// Size, 0.5–2.0.
    pub size: f32,
    pub ticklish: f32,
    pub patience: f32,
    pub energy: f32,
    pub on_desktop: bool,
    pub level: u32,
    pub xp: u32,
    pub xp_day: i64,
    pub xp_today: u32,
}

impl PetSave {
    pub fn adopt(id: u64, species: Species, coat: &str, rng: &mut fastrand::Rng) -> Self {
        // Each individual rolls its own personality near the species default (PLAN §1.1).
        let p = species.personality();
        let jitter =
            |v: f32, rng: &mut fastrand::Rng| (v + (rng.f32() - 0.5) * 0.3).clamp(0.0, 1.0);
        Self {
            id,
            species: species.id().into(),
            name: species.default_name().into(),
            you: species.default_you().into(),
            coat: coat.into(),
            chonk: 0.33,
            size: 1.0,
            ticklish: jitter(p.ticklish, rng),
            patience: jitter(p.patience, rng),
            energy: jitter(p.energy, rng),
            on_desktop: true,
            level: 1,
            xp: 0,
            xp_day: 0,
            xp_today: 0,
        }
    }

    pub fn species(&self) -> Option<Species> {
        Species::from_id(&self.species)
    }

    pub fn personality(&self) -> Personality {
        Personality {
            ticklish: self.ticklish,
            patience: self.patience,
            energy: self.energy,
        }
    }

    pub fn affinity(&self) -> Affinity {
        Affinity {
            level: self.level.max(1),
            xp: self.xp,
            day: self.xp_day,
            gained_today: self.xp_today,
        }
    }

    pub fn set_affinity(&mut self, a: &Affinity) {
        self.level = a.level;
        self.xp = a.xp;
        self.xp_day = a.day;
        self.xp_today = a.gained_today;
    }

    /// Clamp everything a hand-edited or old file could get wrong.
    pub fn sanitise(&mut self) {
        let clamp01 = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.5
            }
        };
        self.chonk = clamp01(self.chonk);
        self.size = if self.size.is_finite() {
            self.size.clamp(0.5, 2.0)
        } else {
            1.0
        };
        self.ticklish = clamp01(self.ticklish);
        self.patience = clamp01(self.patience);
        self.energy = clamp01(self.energy);
        self.name = self.name.chars().take(crate::lines::MAX_NAME).collect();
        self.you = self.you.chars().take(crate::lines::MAX_YOU).collect();
        self.level = self.level.max(1);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pets {
    pub format: u32,
    pub next_id: u64,
    pub pets: Vec<PetSave>,
}

impl Pets {
    pub fn sanitise(&mut self) {
        self.pets.retain(|p| p.species().is_some());
        for p in &mut self.pets {
            p.sanitise();
        }
        let max = self.pets.iter().map(|p| p.id).max().unwrap_or(0);
        self.next_id = self.next_id.max(max + 1);
    }
}

/// Read a RON file; `None` if it does not exist. A broken file is moved
/// aside as `<name>.bad` and reported.
pub fn load<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, String> {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let parsed = if meta.len() > MAX_FILE {
        Err("file is too large".to_string())
    } else {
        fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|text| ron::from_str(&text).map_err(|e| e.to_string()))
    };
    match parsed {
        Ok(v) => Ok(Some(v)),
        Err(e) => {
            let bad = path.with_extension("ron.bad");
            let _ = fs::rename(path, &bad);
            Err(format!(
                "{} could not be read ({e}); kept it as {} and started fresh",
                path.display(),
                bad.display()
            ))
        }
    }
}

/// Write `value` atomically: a temporary file, then a rename.
pub fn save<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let text = ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::default())
        .map_err(|e| e.to_string())?;
    let tmp = path.with_extension("ron.tmp");
    fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("chonkgene-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn settings_round_trip() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("settings.ron");
        let s = Settings {
            volume: 33,
            squabble: 2,
            ..Settings::default()
        };
        save(&path, &s).unwrap();
        assert_eq!(load::<Settings>(&path).unwrap(), Some(s));
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        let dir = temp_dir("missing");
        assert_eq!(load::<Settings>(&dir.join("nope.ron")).unwrap(), None);
    }

    #[test]
    fn a_broken_file_is_kept_aside() {
        let dir = temp_dir("broken");
        let path = dir.join("pets.ron");
        fs::write(&path, "(pets: [oops").unwrap();
        assert!(load::<Pets>(&path).is_err());
        assert!(!path.exists());
        assert!(dir.join("pets.ron.bad").exists());
    }

    #[test]
    fn old_files_get_new_fields_by_default() {
        let dir = temp_dir("default");
        let path = dir.join("settings.ron");
        fs::write(&path, "(sound: false)").unwrap();
        let s = load::<Settings>(&path).unwrap().unwrap();
        assert!(!s.sound);
        assert_eq!(s.on_desktop, 6);
    }

    #[test]
    fn hand_edited_values_are_clamped() {
        let mut rng = fastrand::Rng::with_seed(1);
        let mut p = PetSave::adopt(1, Species::Shiba, "red", &mut rng);
        p.chonk = 9.0;
        p.size = f32::NAN;
        p.name = "名".repeat(40);
        p.sanitise();
        assert_eq!(p.chonk, 1.0);
        assert_eq!(p.size, 1.0);
        assert_eq!(p.name.chars().count(), crate::lines::MAX_NAME);
    }
}
