//! The words on screen in the user's language (PLAN §8, Roger's
//! twenty-third pass, item 1): Traditional Chinese, Japanese or English.
//!
//! The Chinese is written in the code as it always was, and is the key:
//! `tr("摸摸頭")` gives the string in the language chosen, looked up in
//! `i18n/<lang>/LC_MESSAGES/chonkgene.po` (gettext, the same files the
//! settings window's `@tr` reads through Slint). A string with no
//! translation yet shows in Chinese.
//!
//! The language is one for the whole process, chosen once at start (and
//! again when the user changes it): [`set`].

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// A language the pets speak.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Lang {
    #[default]
    ZhTw,
    Ja,
    En,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::ZhTw, Lang::Ja, Lang::En];

    /// As saved in settings and as Slint's bundled translations name it.
    pub fn id(self) -> &'static str {
        match self {
            Lang::ZhTw => "zh-TW",
            Lang::Ja => "ja",
            Lang::En => "en",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.id() == id)
    }

    /// Its own name, written in itself (for the choice in settings).
    pub fn name(self) -> &'static str {
        match self {
            Lang::ZhTw => "繁體中文",
            Lang::Ja => "日本語",
            Lang::En => "English",
        }
    }

    /// The language for a system locale such as `ja_JP.UTF-8`, `zh-TW`
    /// or `en-US` (Roger: follow the computer's): Japanese for Japanese,
    /// Chinese for any Chinese, English for anything else.
    pub fn for_locale(locale: &str) -> Self {
        let l = locale.trim().to_ascii_lowercase();
        if l.starts_with("ja") {
            Lang::Ja
        } else if l.starts_with("zh") {
            Lang::ZhTw
        } else {
            Lang::En
        }
    }

    fn po(self) -> Option<&'static str> {
        match self {
            Lang::ZhTw => None,
            Lang::Ja => Some(include_str!("../../../i18n/ja/LC_MESSAGES/chonkgene.po")),
            Lang::En => Some(include_str!("../../../i18n/en/LC_MESSAGES/chonkgene.po")),
        }
    }
}

type Table = HashMap<&'static str, &'static str>;

static CURRENT: RwLock<Lang> = RwLock::new(Lang::ZhTw);

fn tables() -> &'static HashMap<Lang, Table> {
    static TABLES: OnceLock<HashMap<Lang, Table>> = OnceLock::new();
    TABLES.get_or_init(|| {
        Lang::ALL
            .into_iter()
            .filter_map(|l| l.po().map(|po| (l, parse(po))))
            .collect()
    })
}

/// Use `lang` from now on.
pub fn set(lang: Lang) {
    if let Ok(mut l) = CURRENT.write() {
        *l = lang;
    }
}

pub fn current() -> Lang {
    CURRENT.read().map_or(Lang::ZhTw, |l| *l)
}

/// `zh` in the current language (itself in Chinese, or with no
/// translation yet).
pub fn tr(zh: &'static str) -> &'static str {
    tr_in(current(), zh)
}

pub fn tr_in(lang: Lang, zh: &'static str) -> &'static str {
    tables()
        .get(&lang)
        .and_then(|t| t.get(zh))
        .copied()
        .unwrap_or(zh)
}

/// As [`tr`], then each `{key}` replaced by its value.
pub fn trf(zh: &'static str, args: &[(&str, &str)]) -> String {
    let mut s = tr(zh).to_string();
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// Every msgid in a language's table (tests: each string the code asks
/// for has an entry).
pub fn msgids(lang: Lang) -> Vec<&'static str> {
    lang.po()
        .map(|po| parse_all(po).into_iter().map(|(id, _)| id).collect())
        .unwrap_or_default()
}

/// msgid → msgstr, leaving out entries not translated yet.
fn parse(po: &'static str) -> Table {
    parse_all(po)
        .into_iter()
        .filter(|(_, s)| !s.is_empty())
        .map(|(id, s)| (id, &*Box::leak(s.into_boxed_str())))
        .collect()
}

/// Every (msgid, msgstr) of a .po file, the header left out. msgids are
/// leaked once (the tables live as long as the process).
fn parse_all(po: &str) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    let (mut id, mut msg): (Option<String>, Option<String>) = (None, None);
    let mut into_id = false;
    let mut flush = |id: &mut Option<String>, msg: &mut Option<String>| {
        if let (Some(i), Some(m)) = (id.take(), msg.take())
            && !i.is_empty()
        {
            out.push((&*Box::leak(i.into_boxed_str()), m));
        }
    };
    for line in po.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("msgid ") {
            flush(&mut id, &mut msg);
            id = Some(unquote(rest));
            into_id = true;
        } else if let Some(rest) = line.strip_prefix("msgstr ") {
            msg = Some(unquote(rest));
            into_id = false;
        } else if line.starts_with('"') {
            let more = unquote(line);
            let to = if into_id { &mut id } else { &mut msg };
            if let Some(s) = to {
                s.push_str(&more);
            }
        }
    }
    flush(&mut id, &mut msg);
    out
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let inner = s
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(s);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_locale_picks_its_language() {
        assert_eq!(Lang::for_locale("ja_JP.UTF-8"), Lang::Ja);
        assert_eq!(Lang::for_locale("zh-TW"), Lang::ZhTw);
        assert_eq!(Lang::for_locale("zh_CN.UTF-8"), Lang::ZhTw);
        assert_eq!(Lang::for_locale("en-US"), Lang::En);
        assert_eq!(Lang::for_locale("de_DE"), Lang::En);
        assert_eq!(Lang::for_locale("C"), Lang::En);
    }

    #[test]
    fn po_entries_are_read_with_their_escapes_and_continuations() {
        let po = r#"
msgid ""
msgstr "Content-Type: text/plain; charset=UTF-8\n"

msgid "摸摸頭"
msgstr "Pat"

# a comment
msgid "還有 {n} 份"
msgstr ""
"{n} "
"left"

msgid "說\"嗨\""
msgstr ""
"#;
        let all = parse_all(po);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0], ("摸摸頭", "Pat".to_string()));
        assert_eq!(all[1], ("還有 {n} 份", "{n} left".to_string()));
        assert_eq!(all[2].0, "說\"嗨\"");
        // Not translated yet: left out of the table, so it shows in Chinese.
        assert!(!parse(po).contains_key("說\"嗨\""));
    }

    #[test]
    fn chinese_is_its_own_translation() {
        assert_eq!(tr_in(Lang::ZhTw, "摸摸頭"), "摸摸頭");
    }
}
