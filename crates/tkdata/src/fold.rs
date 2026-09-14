//! Key folding, an exact mirror of `fold_key` / `fold_en` in tools/kcommon.py.
//!
//! The dictionary was built with these functions, so lookups at runtime must
//! fold input words identically.

use unicode_normalization::UnicodeNormalization;

const STRIP_MARKS_LANGS: &[&str] = &[
    "ar", "hit", "be", "la", "pi", "got", "grc", "uk", "sa", "sh", "ofs", "cu", "goh", "arc", "ang", "odt", "akk",
    "sr", "egy", "he", "fa", "prg", "sux", "ps", "ur", "bg", "sla-pro", "syc", "non", "lv", "mk", "osx", "ain",
    "ru", "xcl", "gem-pro", "orv", "lt", "ine-pro",
];

const COMBINING_TO_STRIP: &[char] = &[
    '\u{0311}', '\u{030C}', '\u{0302}', '\u{0304}', '\u{0323}', '\u{0303}', '\u{0331}', '\u{0306}', '\u{0300}',
    '\u{0301}', '\u{0308}',
];

fn is_superscript_digit(c: char) -> bool {
    matches!(c, '\u{00B9}' | '\u{00B2}' | '\u{00B3}' | '\u{2070}' | '\u{2074}'..='\u{2079}')
}

fn is_harakat(c: char) -> bool {
    matches!(c, '\u{0610}'..='\u{061A}' | '\u{064B}'..='\u{065F}' | '\u{0670}' | '\u{06D6}'..='\u{06ED}' | '\u{0640}')
}

fn is_niqqud(c: char) -> bool {
    matches!(c, '\u{0591}'..='\u{05C7}')
}

pub fn strips_marks(lang: &str) -> bool {
    STRIP_MARKS_LANGS.contains(&lang)
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = true;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    while out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Fold a source-language word into its dictionary key form.
pub fn fold_key(lang: &str, word: &str) -> String {
    let w: String = word
        .trim()
        .chars()
        .filter(|c| !is_superscript_digit(*c))
        .map(|c| match c {
            '\u{2019}' | '\u{02BC}' | '`' => '\'',
            c => c,
        })
        .collect();
    let w: String = w.nfc().collect::<String>().to_lowercase();
    let w = if strips_marks(lang) {
        let keep: &[char] = match lang {
            "lv" => &['\u{0304}', '\u{030C}'],
            "lt" => &['\u{030C}'],
            _ => &[],
        };
        let d: String = w.nfd().filter(|c| !(COMBINING_TO_STRIP.contains(c) && !keep.contains(c))).collect();
        let mut w: String = d.nfc().collect();
        if matches!(lang, "ar" | "fa" | "ur" | "ps") {
            w = w.chars().filter(|c| !is_harakat(*c)).collect();
        }
        if matches!(lang, "he" | "arc" | "syc") {
            w = w.chars().filter(|c| !is_niqqud(*c)).collect();
        }
        w
    } else if lang == "vi" {
        w
    } else {
        w.chars().filter(|c| !matches!(c, '\u{0301}' | '\u{0300}' | '\u{0304}')).collect()
    };
    collapse_ws(&w)
}

/// Fold an English key.
pub fn fold_en(word: &str) -> String {
    let w: String = word.trim().chars().map(|c| if c == '\u{2019}' { '\'' } else { c }).collect();
    let w = w.nfc().collect::<String>().to_lowercase();
    let w = collapse_ws(&w);
    w.trim_matches(|c| matches!(c, ' ' | '.' | ',' | ';' | ':' | '!' | '?' | '"')).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folds() {
        assert_eq!(fold_key("ru", "до́м"), "дом");
        assert_eq!(fold_key("la", "amāre"), "amare");
        assert_eq!(fold_key("pl", "Dom"), "dom");
        assert_eq!(fold_key("de", "Straße"), "straße");
        assert_eq!(fold_key("lv", "māja"), "māja");
        assert_eq!(fold_en(" The House. "), "the house");
    }
}
