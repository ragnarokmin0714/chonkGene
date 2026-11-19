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
use chonkgene_core::i18n::{self, Lang};
use chonkgene_core::mind::Affinity;
use chonkgene_core::species::Species;
use serde::{Deserialize, Serialize};

pub const PORTABLE_DIR: &str = "chonkGene-data";
const MAX_FILE: u64 = 1024 * 1024;
/// 2: the default desktop limit went from 6 to 7 (`Settings::upgrade`).
const FORMAT: u32 = 2;

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
    /// The pets react to typing and to a still mouse (input timing only,
    /// PLAN §1.3, §6).
    pub react_to_input: bool,
    pub start_at_login: bool,
    /// How many pets are on the desktop at once; no upper limit (PLAN §1.1).
    pub on_desktop: u32,
    /// Set once the first run has been handled.
    pub welcomed: bool,
    /// The language, as `i18n::Lang::id`; empty: the computer's (Roger,
    /// 2026-11-17: follow the computer by default).
    pub language: String,
    /// 「讓牠們隨機輪班」: about every this many minutes one pet out on the
    /// desktop and one resting in 寵物窩, each picked at random, change
    /// places; 0 never (the default, Roger 2026-11-19). One of
    /// [`ROTATE_CHOICES`].
    pub rotate_minutes: u32,
}

/// The intervals 「讓牠們隨機輪班」 offers, minutes (0: off).
pub const ROTATE_CHOICES: [u32; 4] = [0, 30, 60, 180];

impl Default for Settings {
    fn default() -> Self {
        Self {
            format: FORMAT,
            sound: true,
            volume: 70,
            irrational: true,
            squabble: 0,
            hide_in_fullscreen: true,
            react_to_input: true,
            start_at_login: false,
            on_desktop: 7,
            welcomed: false,
            language: String::new(),
            rotate_minutes: 0,
        }
    }
}

impl Settings {
    /// The language to show: the one chosen, or the computer's.
    pub fn lang(&self) -> chonkgene_core::i18n::Lang {
        use chonkgene_core::i18n::Lang;
        Lang::from_id(&self.language)
            .unwrap_or_else(|| Lang::for_locale(&sys_locale::get_locale().unwrap_or_default()))
    }

    /// Bring a file written by an older version up to date; true if it
    /// changed. Format 1 defaulted to six pets on the desktop; a 6 still
    /// there is that old default (Roger: seven by default), so it becomes
    /// 7. Any other number was chosen and is kept.
    pub fn upgrade(&mut self) -> bool {
        if self.format >= FORMAT {
            return false;
        }
        if self.format < 2 && self.on_desktop == 6 {
            self.on_desktop = 7;
        }
        self.format = FORMAT;
        true
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
    /// 常駐: never sent to rest in 寵物窩 to make room for another (換班)
    /// or by 隨機輪班; only by hand or by a lower limit, and then last.
    /// Missing from saves before 2026-11-19 (without the default the whole
    /// file was set aside as broken).
    #[serde(default)]
    pub pinned: bool,
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
            pinned: false,
            level: 1,
            xp: 0,
            xp_day: 0,
            xp_today: 0,
        }
    }

    pub fn species(&self) -> Option<Species> {
        Species::from_id(&self.species)
    }

    /// A name or 「牠怎麼叫你」 still at its species' default, in whatever
    /// language it was saved (the pet was adopted in Chinese, say), follows
    /// the language shown now; one typed in by hand stays as it is. True if
    /// anything changed.
    pub fn localize_defaults(&mut self) -> bool {
        self.localize_defaults_in(i18n::current())
    }

    /// [`Self::localize_defaults`] for language `lang`.
    pub fn localize_defaults_in(&mut self, lang: Lang) -> bool {
        let Some(species) = self.species() else {
            return false;
        };
        let mut changed = false;
        for (field, key) in [
            (&mut self.name, species.default_name_zh()),
            (&mut self.you, species.default_you_zh()),
        ] {
            let now = i18n::tr_in(lang, key);
            let a_default = Lang::ALL
                .iter()
                .any(|&l| field.as_str() == i18n::tr_in(l, key));
            if a_default && field.as_str() != now {
                *field = now.into();
                changed = true;
            }
        }
        changed
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

/// What calling a pet out did to the desktop (see [`Pets::call_out`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Room {
    /// There was room.
    Free,
    /// This pet went to rest in 寵物窩 to make room (換班).
    Swapped(u64),
    /// Every other pet out is 常駐: one more out than the limit (the
    /// caller raises it to match).
    Over,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pets {
    pub format: u32,
    pub next_id: u64,
    pub pets: Vec<PetSave>,
}

impl Pets {
    /// How many are out on the desktop (not resting in 寵物窩).
    pub fn out(&self) -> usize {
        self.pets.iter().filter(|p| p.on_desktop).count()
    }

    /// Make as many pets as `cap` allows be out on the desktop: over it, the
    /// last ones out in the order go to rest in 寵物窩 (常駐 ones only once
    /// no other is left out); under it, resting ones come out, first in the
    /// order first. True if any pet moved. (Both processes run it on a
    /// change of 「同時在桌面上的寵物」, so their lists agree.)
    pub fn fit_to(&mut self, cap: u32) -> bool {
        let cap = cap as usize;
        let mut out = self.out();
        let mut moved = false;
        for pinned in [false, true] {
            for p in self.pets.iter_mut().rev() {
                if out <= cap {
                    break;
                }
                if p.on_desktop && p.pinned == pinned {
                    p.on_desktop = false;
                    (out, moved) = (out - 1, true);
                }
            }
        }
        for p in &mut self.pets {
            if out >= cap {
                break;
            }
            if !p.on_desktop {
                p.on_desktop = true;
                (out, moved) = (out + 1, true);
            }
        }
        moved
    }

    /// Call pet `id` out onto a desktop that holds `cap` (by hand, or just
    /// adopted). With the desktop full, the last one out in the order that
    /// is not 常駐 goes to rest in 寵物窩 in its place (換班, Roger
    /// 2026-11-19: no longer a silently higher limit). Both processes run
    /// it, so their lists agree.
    pub fn call_out(&mut self, id: u64, cap: u32) -> Room {
        let Some(i) = self.pets.iter().position(|p| p.id == id) else {
            return Room::Free;
        };
        self.pets[i].on_desktop = true;
        if self.out() <= cap as usize {
            return Room::Free;
        }
        match self
            .pets
            .iter_mut()
            .rev()
            .find(|p| p.on_desktop && !p.pinned && p.id != id)
        {
            Some(p) => {
                p.on_desktop = false;
                Room::Swapped(p.id)
            }
            None => Room::Over,
        }
    }

    /// Adopt `save` onto a desktop that holds `cap`: it comes out at once,
    /// making room as [`Pets::call_out`] does (Roger 2026-11-19: one just
    /// adopted rested unseen in 寵物窩 when the desktop was full). Last in
    /// the order.
    pub fn adopt(&mut self, mut save: PetSave, cap: u32) -> Room {
        let id = save.id;
        save.on_desktop = false;
        self.pets.push(save);
        self.call_out(id, cap)
    }

    /// Move pet `id` `by` places in the order (−1 earlier, +1 later),
    /// within the list. True if it moved.
    pub fn shift(&mut self, id: u64, by: i32) -> bool {
        let Some(i) = self.pets.iter().position(|p| p.id == id) else {
            return false;
        };
        let to = (i as i64 + i64::from(by)).clamp(0, self.pets.len() as i64 - 1) as usize;
        if to == i {
            return false;
        }
        let p = self.pets.remove(i);
        self.pets.insert(to, p);
        true
    }

    /// Put the pets in the order of `ids`; any not named keep their order
    /// after those named (a pet adopted meanwhile). True if it changed.
    pub fn reorder(&mut self, ids: &[u64]) -> bool {
        let rank = |id: u64| ids.iter().position(|&i| i == id).unwrap_or(usize::MAX);
        let before: Vec<u64> = self.pets.iter().map(|p| p.id).collect();
        // A stable sort: those not named stay in their order, at the end.
        self.pets.sort_by_key(|p| rank(p.id));
        self.pets.iter().map(|p| p.id).ne(before)
    }

    /// The order, as ids (for [`Pets::reorder`] in the other process).
    pub fn order(&self) -> Vec<u64> {
        self.pets.iter().map(|p| p.id).collect()
    }

    /// 隨機輪班: one pet out (not 常駐, and `free` to go: not in your hands,
    /// not in the middle of something) and one resting in 寵物窩, each
    /// picked at random; none if either is missing.
    pub fn pick_shift(
        &self,
        rng: &mut fastrand::Rng,
        free: impl Fn(u64) -> bool,
    ) -> Option<(u64, u64)> {
        let going: Vec<u64> = self
            .pets
            .iter()
            .filter(|p| p.on_desktop && !p.pinned && free(p.id))
            .map(|p| p.id)
            .collect();
        let coming: Vec<u64> = self
            .pets
            .iter()
            .filter(|p| !p.on_desktop)
            .map(|p| p.id)
            .collect();
        if going.is_empty() || coming.is_empty() {
            return None;
        }
        Some((
            going[rng.usize(..going.len())],
            coming[rng.usize(..coming.len())],
        ))
    }

    /// Pet `going` goes to rest in 寵物窩 and `coming` comes out in its
    /// place (隨機輪班). True if both were where they should be.
    pub fn swap(&mut self, going: u64, coming: u64) -> bool {
        let ok = self.pets.iter().any(|p| p.id == going && p.on_desktop)
            && self.pets.iter().any(|p| p.id == coming && !p.on_desktop);
        if ok {
            for p in &mut self.pets {
                if p.id == going {
                    p.on_desktop = false;
                } else if p.id == coming {
                    p.on_desktop = true;
                }
            }
        }
        ok
    }

    /// Every pet's untouched default name and 「牠怎麼叫你」 in the language
    /// shown now (see [`PetSave::localize_defaults`]). True if any changed.
    pub fn localize_defaults(&mut self) -> bool {
        self.pets
            .iter_mut()
            .fold(false, |any, p| p.localize_defaults() | any)
    }

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
    #[test]
    fn untouched_names_follow_the_language_and_typed_ones_stay() {
        let mut rng = fastrand::Rng::with_seed(1);
        // Adopted while the program was in Chinese.
        let mut munchkin = PetSave::adopt(1, Species::Munchkin, "cream", &mut rng);
        munchkin.name = i18n::tr_in(Lang::ZhTw, Species::Munchkin.default_name_zh()).into();
        munchkin.you = i18n::tr_in(Lang::ZhTw, Species::Munchkin.default_you_zh()).into();
        assert!(munchkin.localize_defaults_in(Lang::En));
        assert_eq!(munchkin.you, i18n::tr_in(Lang::En, "好朋友"));
        assert_ne!(
            munchkin.you, "好朋友",
            "the English table has no 「好朋友」"
        );
        // On to Japanese from English, and back: still the defaults.
        assert!(munchkin.localize_defaults_in(Lang::Ja));
        assert_eq!(munchkin.name, i18n::tr_in(Lang::Ja, "奶茶"));
        assert!(munchkin.localize_defaults_in(Lang::ZhTw));
        assert_eq!(
            (munchkin.name.as_str(), munchkin.you.as_str()),
            ("奶茶", "好朋友")
        );
        assert!(
            !munchkin.localize_defaults_in(Lang::ZhTw),
            "nothing left to change"
        );
        // Renamed by hand: kept in every language.
        munchkin.name = "小麻糬".into();
        munchkin.localize_defaults_in(Lang::En);
        assert_eq!(munchkin.name, "小麻糬");
    }

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("chonkgene-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Roger changed 「同時在桌面上的寵物」 and nothing happened: a pet
    /// adopted over the limit rested in 寵物窩, and raising the limit left
    /// it there. The limit now moves pets both ways.
    #[test]
    fn the_desktop_limit_sends_pets_to_rest_and_calls_them_back() {
        let mut rng = fastrand::Rng::with_seed(1);
        let mut pets = Pets::default();
        for id in 1..=7 {
            let mut p =
                PetSave::adopt(id, chonkgene_core::species::Species::Shiba, "red", &mut rng);
            p.on_desktop = id <= 6;
            pets.pets.push(p);
        }
        let out = |p: &Pets| -> Vec<u64> {
            p.pets
                .iter()
                .filter(|p| p.on_desktop)
                .map(|p| p.id)
                .collect()
        };
        assert!(pets.fit_to(7));
        assert_eq!(out(&pets), [1, 2, 3, 4, 5, 6, 7]);
        assert!(pets.fit_to(3));
        assert_eq!(out(&pets), [1, 2, 3]);
        assert!(!pets.fit_to(3));
        // One rested by hand comes back out before the later ones.
        pets.pets[1].on_desktop = false;
        assert!(pets.fit_to(4));
        assert_eq!(out(&pets), [1, 2, 3, 4]);
        // A limit over the number of pets brings them all out.
        assert!(pets.fit_to(10));
        assert_eq!(pets.out(), 7);
    }

    fn shibas(n: u64, out: u64) -> Pets {
        let mut rng = fastrand::Rng::with_seed(1);
        let mut pets = Pets::default();
        for id in 1..=n {
            let mut p = PetSave::adopt(id, Species::Shiba, "red", &mut rng);
            p.on_desktop = id <= out;
            pets.pets.push(p);
        }
        pets.next_id = n + 1;
        pets
    }

    fn out_ids(p: &Pets) -> Vec<u64> {
        p.pets
            .iter()
            .filter(|p| p.on_desktop)
            .map(|p| p.id)
            .collect()
    }

    /// Roger, 2026-11-19: with the desktop full, calling one out (or
    /// adopting one) swaps it with the last one out, instead of raising the
    /// limit; 常駐 ones are never the ones swapped.
    #[test]
    fn calling_one_out_onto_a_full_desktop_swaps_it_with_the_last_one_out() {
        let mut pets = shibas(5, 3);
        assert_eq!(pets.call_out(5, 3), Room::Swapped(3));
        assert_eq!(out_ids(&pets), [1, 2, 5]);
        // Room to spare: it just comes out.
        assert_eq!(pets.call_out(4, 4), Room::Free);
        assert_eq!(out_ids(&pets), [1, 2, 4, 5]);
        // The last one out is 常駐: the one before it goes instead.
        let mut pets = shibas(5, 3);
        pets.pets[2].pinned = true;
        assert_eq!(pets.call_out(4, 3), Room::Swapped(2));
        assert_eq!(out_ids(&pets), [1, 3, 4]);
        // Every other one out is 常駐: one over the limit.
        for p in &mut pets.pets {
            p.pinned = p.on_desktop;
        }
        assert_eq!(pets.call_out(5, 3), Room::Over);
        assert_eq!(pets.out(), 4);
    }

    #[test]
    fn a_pet_adopted_onto_a_full_desktop_comes_out_at_once() {
        let mut pets = shibas(3, 3);
        let mut rng = fastrand::Rng::with_seed(2);
        let new = PetSave::adopt(4, Species::Owl, "brown", &mut rng);
        assert_eq!(pets.adopt(new, 3), Room::Swapped(3));
        assert_eq!(out_ids(&pets), [1, 2, 4]);
        assert_eq!(pets.order(), [1, 2, 3, 4], "last in the order");
        let new = PetSave::adopt(5, Species::Chick, "yellow", &mut rng);
        assert_eq!(pets.adopt(new, 7), Room::Free);
        assert_eq!(out_ids(&pets), [1, 2, 4, 5]);
    }

    /// 上場順序: the order decides who comes out first and who rests first.
    #[test]
    fn the_order_can_be_changed_and_the_limit_follows_it() {
        let mut pets = shibas(4, 4);
        assert!(pets.shift(4, -1));
        assert!(pets.shift(4, -1));
        assert_eq!(pets.order(), [1, 4, 2, 3]);
        assert!(!pets.shift(1, -1), "already first");
        assert!(!pets.shift(3, 1), "already last");
        assert!(pets.fit_to(2));
        assert_eq!(out_ids(&pets), [1, 4]);
        // The other process takes the order as ids; one it does not know
        // of (adopted meanwhile) stays at the end.
        let mut other = shibas(5, 4);
        assert!(other.reorder(&pets.order()));
        assert_eq!(other.order(), [1, 4, 2, 3, 5]);
        assert!(!other.reorder(&[1, 4, 2, 3, 5]));
    }

    #[test]
    fn a_lower_limit_rests_pinned_pets_last() {
        let mut pets = shibas(4, 4);
        pets.pets[3].pinned = true;
        assert!(pets.fit_to(2));
        assert_eq!(out_ids(&pets), [1, 4]);
        assert!(pets.fit_to(0));
        assert_eq!(pets.out(), 0);
    }

    /// 隨機輪班: never a 常駐 one or one busy, and everyone gets a turn.
    #[test]
    fn a_random_shift_swaps_a_free_pet_out_for_a_resting_one() {
        let mut pets = shibas(6, 3);
        pets.pets[0].pinned = true;
        let mut rng = fastrand::Rng::with_seed(7);
        let (mut went, mut came) = (Vec::new(), Vec::new());
        for _ in 0..200 {
            let (going, coming) = pets.pick_shift(&mut rng, |id| id != 2).unwrap();
            assert!(going != 1 && going != 2, "pinned or busy: {going}");
            assert!(pets.swap(going, coming));
            assert_eq!(pets.out(), 3);
            went.push(going);
            came.push(coming);
        }
        for id in 3..=6 {
            assert!(
                came.contains(&id) && went.contains(&id),
                "{id} never had a turn"
            );
        }
        // Nobody resting, or nobody free to go: no shift.
        let all_out = shibas(3, 3);
        assert_eq!(all_out.pick_shift(&mut rng, |_| true), None);
        assert_eq!(pets.pick_shift(&mut rng, |_| false), None);
        // Stale picks (the other process moved first) do nothing.
        assert!(!pets.swap(1, 1));
    }

    /// A save from before 「一直待在桌面上」 has no `pinned`: it must still
    /// load (it was set aside as broken, every pet gone from the desktop).
    #[test]
    fn a_save_from_before_pinning_still_loads() {
        let dir = temp_dir("unpinned");
        let path = dir.join("pets.ron");
        fs::write(
            &path,
            r#"(format: 0, next_id: 2, pets: [(id: 1, species: "shiba", name: "阿柴",
            you: "主人", coat: "red", chonk: 0.33, size: 1.0, ticklish: 0.5,
            patience: 0.5, energy: 0.5, on_desktop: true, level: 2, xp: 10,
            xp_day: 0, xp_today: 0)])"#,
        )
        .unwrap();
        let pets = load::<Pets>(&path).unwrap().unwrap();
        assert_eq!(pets.pets.len(), 1);
        assert!(!pets.pets[0].pinned);
        assert_eq!(pets.pets[0].level, 2);
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
        assert_eq!(s.on_desktop, 7);
    }

    /// Roger: seven on the desktop by default. A format-1 file still on the
    /// old default of six moves to seven; a number chosen by hand stays.
    #[test]
    fn the_old_default_of_six_becomes_seven() {
        let mut old = Settings {
            format: 1,
            on_desktop: 6,
            ..Settings::default()
        };
        assert!(old.upgrade());
        assert_eq!((old.format, old.on_desktop), (FORMAT, 7));
        assert!(!old.upgrade());
        let mut chosen = Settings {
            format: 1,
            on_desktop: 3,
            ..Settings::default()
        };
        assert!(chosen.upgrade());
        assert_eq!(chosen.on_desktop, 3);
        let mut new = Settings {
            on_desktop: 6,
            ..Settings::default()
        };
        assert!(!new.upgrade());
        assert_eq!(new.on_desktop, 6);
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
