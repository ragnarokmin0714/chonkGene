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
    /// 「大家怎麼叫你」 (Roger, 2026-11-25): what every pet whose own
    /// 「牠怎麼叫你」 nobody changed calls you, pets adopted later too;
    /// empty, each keeps its species' default. See [`PetSave::calls_you`].
    pub you: String,
    /// 「動作要點幾下」 (PLAN §8, Roger's twenty-seventh pass, item 2): 2,
    /// the default, a double tap on a spot does what that spot does (its
    /// head a pat, a front paw 握手, …) and one tap is a poke; 1, one tap
    /// does it, pokes and tickles only on the belly and cheeks.
    pub taps: u8,
}

/// The intervals 「讓牠們隨機輪班」 offers, minutes (0: off). 15 minutes
/// added (Roger, 2026-11-20: 「時間都太久了」).
pub const ROTATE_CHOICES: [u32; 5] = [0, 15, 30, 60, 180];

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
            you: String::new(),
            taps: 2,
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

    /// Clamp what a hand-edited file could get wrong.
    pub fn sanitise(&mut self) {
        self.you = clean_you(&self.you);
        self.taps = self.taps.clamp(1, 2);
    }
}

/// What was typed for 「牠怎麼叫你」 or 「大家怎麼叫你」 as it is kept:
/// trimmed, at most [`crate::lines::MAX_YOU`] characters.
pub fn clean_you(typed: &str) -> String {
    let cut: String = typed.trim().chars().take(crate::lines::MAX_YOU).collect();
    cut.trim().to_string()
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
    /// The fox in her human form (人型形態, PLAN §8); her hairdo and hair
    /// colour in it (unknown ones are the first: 綁單馬尾, white); whether
    /// she changes by herself. Missing from saves before 2026-11-24. Only
    /// the pet process changes `human` (she changes by herself, or from her
    /// menu); the settings window sets the rest.
    #[serde(default)]
    pub human: bool,
    #[serde(default)]
    pub hairdo: String,
    #[serde(default)]
    pub hair: String,
    #[serde(default)]
    pub form_lock: FormLock,
    /// Its 「牠怎麼叫你」 was typed in by hand even if it reads as its
    /// species' default (asked to keep 「主人」 while 「大家怎麼叫你」 is
    /// set): it does not follow the shared one. Missing from saves before
    /// 2026-11-25, where anything not a default was typed in anyway.
    #[serde(default)]
    pub own_you: bool,
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
            human: false,
            hairdo: String::new(),
            hair: String::new(),
            form_lock: FormLock::Free,
            own_you: false,
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
        // A name it had by default before (the fox's 小九): today's.
        if species
            .former_names_zh()
            .iter()
            .any(|old| is_default(&self.name, old))
        {
            self.name = i18n::tr_in(lang, species.default_name_zh()).into();
            changed = true;
        }
        let mut fields = vec![(&mut self.name, species.default_name_zh())];
        if !self.own_you {
            fields.push((&mut self.you, species.default_you_zh()));
        }
        for (field, key) in fields {
            let now = i18n::tr_in(lang, key);
            if is_default(field, key) && field.as_str() != now {
                *field = now.into();
                changed = true;
            }
        }
        changed
    }

    /// Its 「牠怎麼叫你」 is still its species' default, in any language:
    /// nobody changed it, so it follows 「大家怎麼叫你」 when that is set.
    pub fn you_untouched(&self) -> bool {
        !self.own_you
            && self
                .species()
                .is_some_and(|s| is_default(&self.you, s.default_you_zh()))
    }

    /// What it calls you: `everyone` (「大家怎麼叫你」) if that is set and
    /// its own was never changed, else its own.
    pub fn calls_you<'a>(&'a self, everyone: &'a str) -> &'a str {
        if !everyone.is_empty() && self.you_untouched() {
            everyone
        } else {
            &self.you
        }
    }

    /// Back to its species' default 「牠怎麼叫你」, in the language shown,
    /// so it follows 「大家怎麼叫你」 again.
    pub fn follow_everyone(&mut self) {
        if let Some(s) = self.species() {
            self.you = s.default_you().into();
            self.own_you = false;
        }
    }

    /// Store `typed` (cleaned, not empty) as its own 「牠怎麼叫你」. Its
    /// species' default counts as typed in only while `everyone` is set
    /// (kept apart from the shared one on purpose); otherwise it is just
    /// the default again.
    pub fn set_own_you(&mut self, typed: String, everyone: &str) {
        self.own_you = !everyone.is_empty()
            || !self
                .species()
                .is_some_and(|s| is_default(&typed, s.default_you_zh()));
        self.you = typed;
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

/// `text` is the default `key` (as the code writes it) in some language.
fn is_default(text: &str, key: &'static str) -> bool {
    Lang::ALL.iter().any(|&l| text == i18n::tr_in(l, key))
}

/// Whether the fox changes form by herself (Roger, 2026-11-16: 「她偶爾會
/// 自己變，我也可以叫他維持原型和人型」).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormLock {
    /// 隨牠變: now and then by herself.
    #[default]
    Free,
    /// 維持原型.
    Beast,
    /// 維持人型.
    Human,
}

impl FormLock {
    /// The form it keeps her in, if it keeps her in one.
    pub fn keeps(self) -> Option<bool> {
        match self {
            Self::Free => None,
            Self::Beast => Some(false),
            Self::Human => Some(true),
        }
    }
}

/// Where one pet is, for `Msg::Roster` (see [`Pets::roster`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seat {
    pub id: u64,
    /// Out on the desktop (not resting in 寵物窩).
    pub out: bool,
    /// 常駐.
    pub pinned: bool,
}

/// What calling a pet out did to the desktop (see [`Pets::seat`]).
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

    /// The order kept: those out on the desktop first, then those resting
    /// in 寵物窩, each in their own order (「出場中」 and 「在寵物窩休息」
    /// in the settings, Roger 2026-11-19). The first resting is the next to
    /// come out; the last out is the next to rest.
    pub fn normalize(&mut self) {
        // A stable sort: each side keeps its order.
        self.pets.sort_by_key(|p| !p.on_desktop);
    }

    /// Make as many pets as `cap` allows be out on the desktop: over it, the
    /// last ones out go to rest in 寵物窩 (常駐 ones only once no other is
    /// left out), first in line to come back; under it, the first resting
    /// come out. True if any pet moved. (Both processes run it on a change
    /// of 「同時在桌面上的寵物」, so their lists agree.)
    pub fn fit_to(&mut self, cap: u32) -> bool {
        self.normalize();
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
        self.normalize();
        moved
    }

    /// Put pet `id` out on the desktop (`out`) or to rest in 寵物窩, just
    /// before pet `before` there (none: last), the desktop holding `cap`:
    /// a pet dragged between 「出場中」 and 「在寵物窩休息」, or within one
    /// (Roger 2026-11-19). Coming out onto a full desktop, the last one out
    /// that is not 常駐 goes to rest in its place, first in line to come
    /// back (換班; no longer a silently higher limit). Both processes end
    /// with the same list (the settings window sends it, `Msg::Roster`).
    pub fn seat(&mut self, id: u64, out: bool, before: Option<u64>, cap: u32) -> Room {
        self.normalize();
        let Some(i) = self.pets.iter().position(|p| p.id == id) else {
            return Room::Free;
        };
        // Dropped on itself: it stays where it is.
        if before == Some(id) && self.pets[i].on_desktop == out {
            return Room::Free;
        }
        let mut pet = self.pets.remove(i);
        pet.on_desktop = out;
        let at = before
            .and_then(|b| {
                self.pets
                    .iter()
                    .position(|p| p.id == b && p.on_desktop == out)
            })
            .unwrap_or(if out { self.out() } else { self.pets.len() });
        self.pets.insert(at, pet);
        let room = if out && self.out() > cap as usize {
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
        } else {
            Room::Free
        };
        self.normalize();
        room
    }

    /// Call pet `id` out, last of those out (see [`Pets::seat`]).
    pub fn call_out(&mut self, id: u64, cap: u32) -> Room {
        self.seat(id, true, None, cap)
    }

    /// Adopt `save` onto a desktop that holds `cap`: it comes out at once,
    /// last of those out, making room as [`Pets::seat`] does (Roger
    /// 2026-11-19: one just adopted rested unseen in 寵物窩 when the
    /// desktop was full).
    pub fn adopt(&mut self, mut save: PetSave, cap: u32) -> Room {
        let id = save.id;
        save.on_desktop = false;
        self.pets.push(save);
        self.call_out(id, cap)
    }

    /// 常駐 or not (double-click in 「出場中」): never the one sent to rest
    /// to make room, nor by 隨機輪班. True if it changed.
    pub fn pin(&mut self, id: u64, pinned: bool) -> bool {
        self.pets
            .iter_mut()
            .find(|p| p.id == id && p.pinned != pinned)
            .map(|p| p.pinned = pinned)
            .is_some()
    }

    /// Who is where, in order (for the other process, `Msg::Roster`).
    pub fn roster(&self) -> Vec<Seat> {
        self.pets
            .iter()
            .map(|p| Seat {
                id: p.id,
                out: p.on_desktop,
                pinned: p.pinned,
            })
            .collect()
    }

    /// Take `roster` from the other process: the order, who is out, who is
    /// 常駐. Pets it does not name (adopted meanwhile) keep their place
    /// after those it does. True if anything changed.
    pub fn take_roster(&mut self, roster: &[Seat]) -> bool {
        let before = self.roster();
        let rank = |id: u64| roster.iter().position(|s| s.id == id).unwrap_or(usize::MAX);
        self.pets.sort_by_key(|p| rank(p.id));
        for p in &mut self.pets {
            if let Some(s) = roster.iter().find(|s| s.id == p.id) {
                p.on_desktop = s.out;
                p.pinned = s.pinned;
            }
        }
        self.normalize();
        self.roster() != before
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

    /// Pet `going` goes to rest in 寵物窩, last in line, and `coming` comes
    /// out in its place in the order (隨機輪班). True if both were where
    /// they should be.
    pub fn swap(&mut self, going: u64, coming: u64) -> bool {
        self.normalize();
        let a = self.pets.iter().position(|p| p.id == going && p.on_desktop);
        let b = self
            .pets
            .iter()
            .position(|p| p.id == coming && !p.on_desktop);
        let (Some(a), Some(b)) = (a, b) else {
            return false;
        };
        self.pets[b].on_desktop = true;
        let came = self.pets.remove(b);
        let mut went = std::mem::replace(&mut self.pets[a], came);
        went.on_desktop = false;
        self.pets.push(went);
        true
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
        self.normalize();
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

    /// Roger, 2026-11-27: the fox's default name is 藻藻 now (after 玉藻前).
    /// A fox still called 小九 in any language was never renamed: she
    /// takes the new name in the language shown; one renamed keeps hers.
    #[test]
    fn a_fox_still_called_by_her_old_default_takes_the_new_one() {
        let mut rng = fastrand::Rng::with_seed(1);
        for (was, lang) in [
            (i18n::tr_in(Lang::ZhTw, "小九"), Lang::ZhTw),
            (i18n::tr_in(Lang::En, "小九"), Lang::Ja),
            (i18n::tr_in(Lang::Ja, "小九"), Lang::En),
        ] {
            let mut fox = PetSave::adopt(1, Species::Fox, "white", &mut rng);
            fox.name = was.into();
            assert!(fox.localize_defaults_in(lang), "{was}");
            assert_eq!(fox.name, i18n::tr_in(lang, "藻藻"), "{was}");
        }
        assert_ne!(i18n::tr_in(Lang::En, "小九"), "小九", "Kyu untranslated");
        assert_eq!(Species::Fox.default_name_zh(), "藻藻");
        let mut fox = PetSave::adopt(1, Species::Fox, "white", &mut rng);
        fox.name = "小狐狸".into();
        fox.localize_defaults_in(Lang::ZhTw);
        assert_eq!(fox.name, "小狐狸");
        // Nobody else had another name before.
        let mut shiba = PetSave::adopt(2, Species::Shiba, "red", &mut rng);
        shiba.name = "小九".into();
        shiba.localize_defaults_in(Lang::ZhTw);
        assert_eq!(shiba.name, "小九", "a Shiba named 小九 by hand");
    }

    #[test]
    fn everyone_calls_you_the_same_unless_one_was_told_otherwise() {
        let mut rng = fastrand::Rng::with_seed(2);
        let mut shiba = PetSave::adopt(1, Species::Shiba, "red", &mut rng);
        // Nothing shared: its species' default.
        assert_eq!(shiba.calls_you(""), shiba.you);
        // Shared, and its own untouched (in any language): the shared one.
        assert_eq!(shiba.calls_you("Roger"), "Roger");
        shiba.you = i18n::tr_in(Lang::Ja, Species::Shiba.default_you_zh()).into();
        assert!(shiba.you_untouched());
        assert_eq!(shiba.calls_you("Roger"), "Roger");
        // Its own typed in: that one, shared or not.
        shiba.you = "老爸".into();
        assert!(!shiba.you_untouched());
        assert_eq!(shiba.calls_you("Roger"), "老爸");
        assert_eq!(shiba.calls_you(""), "老爸");
        // Put back: it follows again.
        shiba.follow_everyone();
        assert_eq!(shiba.calls_you("Roger"), "Roger");
        // Its default typed in while a shared one is set: kept apart, in
        // every language.
        let master = shiba.you.clone();
        shiba.set_own_you(master.clone(), "Roger");
        assert_eq!(shiba.calls_you("Roger"), master);
        shiba.localize_defaults_in(Lang::En);
        assert_eq!(shiba.you, master);
        // Typed with nothing shared, its default is just the default.
        shiba.set_own_you(master.clone(), "");
        assert!(shiba.you_untouched());
        // Saves from before the flag still read.
        let old: PetSave = ron::from_str(
            r#"(id: 1, species: "shiba", name: "阿柴", you: "主人", coat: "red", chonk: 0.33,
            size: 1.0, ticklish: 0.5, patience: 0.5, energy: 0.5, on_desktop: true,
            level: 1, xp: 0, xp_day: 0, xp_today: 0)"#,
        )
        .unwrap();
        assert!(old.you_untouched());
        // A settings file edited by hand is cut to fit a bubble.
        let mut settings = Settings {
            you: "  一二三四五六七八九十  ".into(),
            ..Settings::default()
        };
        settings.sanitise();
        assert_eq!(settings.you, "一二三四五六七八");
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
        // One rested by hand comes back out before the later ones (first
        // in line in 寵物窩, last of those out).
        pets.pets[1].on_desktop = false;
        assert!(pets.fit_to(4));
        assert_eq!(out(&pets), [1, 3, 2, 4]);
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

    fn ids(p: &Pets) -> Vec<u64> {
        p.pets.iter().map(|p| p.id).collect()
    }

    /// Roger, 2026-11-19: with the desktop full, calling one out (or
    /// adopting one) swaps it with the last one out, instead of raising the
    /// limit; 常駐 ones are never the ones swapped.
    #[test]
    fn calling_one_out_onto_a_full_desktop_swaps_it_with_the_last_one_out() {
        let mut pets = shibas(5, 3);
        assert_eq!(pets.call_out(5, 3), Room::Swapped(3));
        assert_eq!(out_ids(&pets), [1, 2, 5]);
        assert_eq!(ids(&pets), [1, 2, 5, 3, 4], "3 first in line to come back");
        // Room to spare: it just comes out, last of those out.
        assert_eq!(pets.call_out(4, 4), Room::Free);
        assert_eq!(out_ids(&pets), [1, 2, 5, 4]);
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
        assert_eq!(ids(&pets), [1, 2, 4, 3]);
        assert_eq!(out_ids(&pets), [1, 2, 4]);
        let new = PetSave::adopt(5, Species::Chick, "yellow", &mut rng);
        assert_eq!(pets.adopt(new, 7), Room::Free);
        assert_eq!(out_ids(&pets), [1, 2, 4, 5]);
    }

    /// 上場順序 by dragging (Roger, 2026-11-19): a name dropped on another
    /// goes just before it, in 「出場中」 or 「在寵物窩休息」; dropped on
    /// the card, last. The limit follows the order.
    #[test]
    fn a_pet_dragged_takes_its_place_and_the_limit_follows_the_order() {
        let mut pets = shibas(4, 4);
        assert_eq!(pets.seat(4, true, Some(2), 4), Room::Free);
        assert_eq!(ids(&pets), [1, 4, 2, 3]);
        assert_eq!(pets.seat(1, false, None, 4), Room::Free);
        assert_eq!(ids(&pets), [4, 2, 3, 1]);
        assert_eq!(out_ids(&pets), [4, 2, 3]);
        // Dropped on itself: stays.
        assert_eq!(pets.seat(2, true, Some(2), 4), Room::Free);
        assert_eq!(ids(&pets), [4, 2, 3, 1]);
        // Into a full desktop, at the front: the last one out rests.
        assert_eq!(pets.seat(1, true, Some(4), 3), Room::Swapped(3));
        assert_eq!(ids(&pets), [1, 4, 2, 3]);
        assert_eq!(out_ids(&pets), [1, 4, 2]);
        // A lower limit rests the last ones out, first in line to come back.
        assert!(pets.fit_to(2));
        assert_eq!(ids(&pets), [1, 4, 2, 3]);
        assert_eq!(out_ids(&pets), [1, 4]);
        assert!(pets.fit_to(3));
        assert_eq!(out_ids(&pets), [1, 4, 2]);
    }

    /// The settings window sends who is where; the pet process takes it.
    #[test]
    fn the_roster_carries_order_place_and_pinning() {
        let mut pets = shibas(4, 2);
        assert!(pets.pin(2, true));
        assert!(!pets.pin(2, true));
        pets.seat(4, true, Some(1), 3);
        let mut other = shibas(5, 2);
        assert!(other.take_roster(&pets.roster()));
        // 5 was adopted meanwhile: it keeps its place after the rest.
        assert_eq!(ids(&other), [4, 1, 2, 3, 5]);
        assert_eq!(out_ids(&other), [4, 1, 2]);
        assert!(other.pets[2].pinned);
        assert!(!other.take_roster(&pets.roster()));
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

    /// 隨機輪班: never a 常駐 one or one busy, and everyone gets a turn;
    /// the one coming out takes the place of the one going.
    #[test]
    fn a_random_shift_swaps_a_free_pet_out_for_a_resting_one() {
        let mut swapped = shibas(5, 3);
        assert!(swapped.swap(2, 5));
        assert_eq!(ids(&swapped), [1, 5, 3, 4, 2]);
        assert_eq!(out_ids(&swapped), [1, 5, 3]);

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
        // Nor the fox's form (2026-11-24): on all fours, free to change.
        let p = &pets.pets[0];
        assert!(!p.human && p.hairdo.is_empty() && p.hair.is_empty());
        assert_eq!(p.form_lock, FormLock::Free);
    }

    /// The fox's form and looks are kept (her human form, PLAN §8), and
    /// a lock keeps her in the form it names.
    #[test]
    fn the_foxs_form_and_looks_are_saved() {
        let dir = temp_dir("fox-form");
        let path = dir.join("pets.ron");
        let mut rng = fastrand::Rng::with_seed(1);
        let mut fox = PetSave::adopt(1, Species::Fox, "snow", &mut rng);
        fox.human = true;
        fox.hairdo = "princess".into();
        fox.hair = "red".into();
        fox.form_lock = FormLock::Human;
        let pets = Pets {
            pets: vec![fox.clone()],
            ..Pets::default()
        };
        save(&path, &pets).unwrap();
        let back = load::<Pets>(&path).unwrap().unwrap();
        assert_eq!(back.pets[0].human, fox.human);
        assert_eq!(back.pets[0].hairdo, "princess");
        assert_eq!(back.pets[0].hair, "red");
        assert_eq!(back.pets[0].form_lock, FormLock::Human);
        assert_eq!(FormLock::Free.keeps(), None);
        assert_eq!(FormLock::Beast.keeps(), Some(false));
        assert_eq!(FormLock::Human.keeps(), Some(true));
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
