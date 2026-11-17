//! Pet lines (assets/lines/<lang>.ron, PLAN §1.11), in the language of the
//! words on screen (`chonkgene_core::i18n`, PLAN §8 twenty-third pass,
//! item 1). A line not written yet in that language is said in Chinese.

use std::collections::HashMap;

use chonkgene_core::i18n::{self, Lang};
use chonkgene_core::mind::Line;
use chonkgene_core::species::Species;

type Table = HashMap<String, HashMap<String, Vec<String>>>;

pub const MAX_NAME: usize = 12;
pub const MAX_YOU: usize = 8;
/// Longest a bubble line may be: as wide as this many Chinese characters
/// (PLAN §1.11), measured in the font, with the longest names filled in.
#[cfg(test)]
const MAX_LINE: usize = 14;

fn source(lang: Lang) -> (&'static str, &'static str) {
    match lang {
        Lang::ZhTw => (
            "assets/lines/zh-TW.ron",
            include_str!("../../../assets/lines/zh-TW.ron"),
        ),
        Lang::Ja => (
            "assets/lines/ja.ron",
            include_str!("../../../assets/lines/ja.ron"),
        ),
        Lang::En => (
            "assets/lines/en.ron",
            include_str!("../../../assets/lines/en.ron"),
        ),
    }
}

pub struct Lines {
    tables: HashMap<Lang, Table>,
}

impl Lines {
    pub fn load() -> Result<Self, String> {
        let mut tables = HashMap::new();
        for lang in Lang::ALL {
            let (path, text) = source(lang);
            let table: Table = ron::from_str(text).map_err(|e| format!("{path}: {e}"))?;
            tables.insert(lang, table);
        }
        Ok(Self { tables })
    }

    fn choices(&self, lang: Lang, species: Species, line: Line) -> Option<&Vec<String>> {
        self.tables
            .get(&lang)?
            .get(species.id())?
            .get(line.key())
            .filter(|c| !c.is_empty())
    }

    /// A random line for `line` in the current language (or in Chinese if
    /// it has none yet), with `{name}` and `{you}` filled in.
    pub fn pick(
        &self,
        species: Species,
        line: Line,
        name: &str,
        you: &str,
        rng: &mut fastrand::Rng,
    ) -> Option<String> {
        let choices = self
            .choices(i18n::current(), species, line)
            .or_else(|| self.choices(Lang::ZhTw, species, line))?;
        let text = choices.get(rng.usize(..choices.len()))?;
        Some(text.replace("{name}", name).replace("{you}", you))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Languages whose lines are written (the others fall back to Chinese
    /// and are not held to these tests yet).
    fn written() -> Vec<Lang> {
        let lines = Lines::load().unwrap();
        Lang::ALL
            .into_iter()
            .filter(|l| lines.tables.get(l).is_some_and(|t| !t.is_empty()))
            .collect()
    }

    #[test]
    fn every_species_has_every_line_in_every_language_written() {
        let lines = Lines::load().unwrap();
        for lang in written() {
            for s in Species::ALL {
                for l in Line::ALL {
                    if l.only_for().is_some_and(|only| only != s) {
                        continue;
                    }
                    assert!(
                        lines.choices(lang, s, l).is_some(),
                        "{}: {} has no {}",
                        lang.id(),
                        s.id(),
                        l.key()
                    );
                }
            }
        }
    }

    #[test]
    fn every_line_fits_a_bubble_with_the_longest_names() {
        let lines = Lines::load().unwrap();
        let mut face = chonkgene_render::text::Face::ui();
        let most = face.measure(&"一".repeat(MAX_LINE), 21.0) + 0.5;
        for lang in written() {
            // The longest names typed in that language: full-width
            // characters, or wide Latin letters.
            let (name, you) = match lang {
                Lang::En => ("W".repeat(MAX_NAME), "W".repeat(MAX_YOU)),
                _ => ("名".repeat(MAX_NAME), "你".repeat(MAX_YOU)),
            };
            for (species, table) in &lines.tables[&lang] {
                assert!(
                    Species::from_id(species).is_some(),
                    "{}: unknown species {species}",
                    lang.id()
                );
                for (key, choices) in table {
                    for text in choices {
                        let filled = text.replace("{name}", &name).replace("{you}", &you);
                        let width = face.measure(&filled, 21.0);
                        assert!(
                            width <= most,
                            "{}: {species}.{key} \"{text}\" is {width:.0} wide (at most {most:.0})",
                            lang.id()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn lines_use_the_glossary() {
        // PLAN §1.11: words that must never appear.
        let banned = ["隱藏", "最小化", "好感度", "愛心值", "怒氣值", "設置"];
        let lines = Lines::load().unwrap();
        for text in lines.tables[&Lang::ZhTw]
            .values()
            .flat_map(|t| t.values().flatten())
        {
            for word in banned {
                assert!(!text.contains(word), "\"{text}\" uses {word}");
            }
        }
    }
}
