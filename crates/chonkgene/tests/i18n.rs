//! Every word the program shows in Chinese has an entry in each language's
//! translation file (PLAN §8, twenty-third pass, item 1): a string added to
//! the code fails here until `python3 tools/i18n/extract.py` brings it in.

use std::path::{Path, PathBuf};

use chonkgene_core::i18n::{Lang, msgids};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if p.is_dir() {
            if !matches!(name, "tests" | "examples" | "target") {
                files(&p, out);
            }
        } else if (name.ends_with(".rs") && name != "i18n.rs") || name.ends_with(".slint") {
            out.push(p);
        }
    }
}

/// The string literal starting at `s` (just after its opening quote), with
/// its escapes as written.
fn literal(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                out.push('\\');
                out.push(chars.next()?);
            }
            '"' => return Some(out),
            c => out.push(c),
        }
    }
    None
}

/// The strings a file asks to be translated, as the extractor finds them:
/// the string may be on the line after the call.
fn asked(path: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    // Comments out.
    let text: String = text
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let slint = path.extension().is_some_and(|e| e == "slint");
    let calls: &[&str] = if slint {
        &["@tr("]
    } else {
        &["tr(", "trf(", "mark("]
    };
    let mut out = Vec::new();
    for call in calls {
        let mut from = 0;
        while let Some(at) = text[from..].find(call) {
            let start = from + at;
            from = start + call.len();
            let before = text[..start].chars().next_back();
            let word = before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '@');
            if !slint && word {
                continue;
            }
            let rest = text[from..].trim_start();
            if let Some(after) = rest.strip_prefix('"')
                && let Some(s) = literal(after)
                && !s.is_empty()
            {
                out.push(s);
            }
        }
    }
    out
}

#[test]
fn every_shown_string_is_in_each_translation_file() {
    let mut paths = Vec::new();
    files(&root().join("crates"), &mut paths);
    let mut wanted: Vec<(String, PathBuf)> = Vec::new();
    for p in &paths {
        for s in asked(p) {
            wanted.push((s, p.clone()));
        }
    }
    assert!(wanted.len() > 100, "found only {} strings", wanted.len());
    for lang in [Lang::Ja, Lang::En] {
        let have = msgids(lang);
        let missing: Vec<String> = wanted
            .iter()
            .filter(|(s, _)| !have.iter().any(|h| *h == s.replace("\\\"", "\"")))
            .map(|(s, p)| format!("{s} ({})", p.display()))
            .collect();
        assert!(
            missing.is_empty(),
            "{}: not in i18n/{}/LC_MESSAGES/chonkgene.po (run tools/i18n/extract.py):\n{}",
            lang.id(),
            lang.id(),
            missing.join("\n")
        );
    }
}
