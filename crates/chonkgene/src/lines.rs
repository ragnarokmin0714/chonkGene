//! Pet lines (assets/lines/zh-TW.ron, PLAN §1.11).

use std::collections::HashMap;

use chonkgene_core::mind::Line;
use chonkgene_core::species::Species;

const ZH_TW: &str = include_str!("../../../assets/lines/zh-TW.ron");
/// Longest a bubble line may be, in characters (PLAN §1.11).
#[cfg(test)]
const MAX_LINE: usize = 14;
pub const MAX_NAME: usize = 12;
pub const MAX_YOU: usize = 8;

pub struct Lines {
    table: HashMap<String, HashMap<String, Vec<String>>>,
}

impl Lines {
    pub fn zh_tw() -> Result<Self, String> {
        let table = ron::from_str(ZH_TW).map_err(|e| format!("assets/lines/zh-TW.ron: {e}"))?;
        Ok(Self { table })
    }

    /// A random line for `line`, with `{name}` and `{you}` filled in.
    pub fn pick(
        &self,
        species: Species,
        line: Line,
        name: &str,
        you: &str,
        rng: &mut fastrand::Rng,
    ) -> Option<String> {
        let choices = self.table.get(species.id())?.get(line.key())?;
        let text = choices.get(rng.usize(..choices.len().max(1)))?;
        Some(text.replace("{name}", name).replace("{you}", you))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_species_has_every_line() {
        let lines = Lines::zh_tw().unwrap();
        for s in Species::ALL {
            for l in Line::ALL {
                let choices = lines.table.get(s.id()).and_then(|t| t.get(l.key()));
                assert!(
                    choices.is_some_and(|c| !c.is_empty()),
                    "{} has no {}",
                    s.id(),
                    l.key()
                );
            }
        }
    }

    #[test]
    fn every_line_fits_a_bubble_with_the_longest_names() {
        let lines = Lines::zh_tw().unwrap();
        let name = "名".repeat(MAX_NAME);
        let you = "你".repeat(MAX_YOU);
        for (species, table) in &lines.table {
            assert!(
                Species::from_id(species).is_some(),
                "unknown species {species}"
            );
            for (key, choices) in table {
                for text in choices {
                    let filled = text.replace("{name}", &name).replace("{you}", &you);
                    let len = filled.chars().count();
                    assert!(
                        len <= MAX_LINE,
                        "{species}.{key} \"{text}\" is {len} characters"
                    );
                }
            }
        }
    }

    #[test]
    fn lines_use_the_glossary() {
        // PLAN §1.11: words that must never appear.
        let banned = ["隱藏", "最小化", "好感度", "愛心值", "怒氣值", "設置"];
        let lines = Lines::zh_tw().unwrap();
        for table in lines.table.values() {
            for text in table.values().flatten() {
                for word in banned {
                    assert!(!text.contains(word), "\"{text}\" uses {word}");
                }
            }
        }
    }
}
