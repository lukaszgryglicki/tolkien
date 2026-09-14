//! Line tokenizer preserving all whitespace and punctuation.

use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Word,
    Number,
    Space,
    Punct,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cap {
    Lower,
    Title,
    Upper,
}

#[derive(Clone, Debug)]
pub struct Token<'a> {
    pub text: &'a str,
    pub kind: Kind,
}

pub fn capitalization(s: &str) -> Cap {
    let mut chars = s.chars().filter(|c| c.is_alphabetic());
    let Some(first) = chars.next() else { return Cap::Lower };
    if !first.is_uppercase() {
        return Cap::Lower;
    }
    let rest: Vec<char> = chars.collect();
    if rest.is_empty() {
        return Cap::Title;
    }
    if rest.iter().all(|c| c.is_uppercase()) {
        Cap::Upper
    } else if rest.iter().all(|c| !c.is_uppercase()) {
        Cap::Title
    } else {
        Cap::Lower
    }
}

pub fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    let mut done = false;
    for c in s.chars() {
        if !done && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            done = true;
        } else {
            out.push(c);
        }
    }
    out
}

pub fn tokenize(line: &str) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    for (_, w) in line.split_word_bound_indices() {
        if w.is_empty() {
            continue;
        }
        let first = w.chars().next().unwrap();
        let kind = if w.chars().all(char::is_whitespace) {
            Kind::Space
        } else if first.is_numeric() && w.chars().all(|c| c.is_numeric() || matches!(c, '.' | ',' | '_')) {
            Kind::Number
        } else if w.chars().any(|c| c.is_alphabetic()) {
            Kind::Word
        } else {
            Kind::Punct
        };
        // merge adjacent punctuation of the same kind for compactness
        if kind == Kind::Punct {
            if let Some(last) = out.last_mut() {
                let last: &mut Token = last;
                if last.kind == Kind::Punct {
                    let start = last.text.as_ptr() as usize - line.as_ptr() as usize;
                    let end = w.as_ptr() as usize - line.as_ptr() as usize + w.len();
                    last.text = &line[start..end];
                    continue;
                }
            }
        }
        // scripts without word spaces: keep the whole run together, it is segmented later
        if kind == Kind::Word && is_unspaced_script(first) {
            if let Some(last) = out.last_mut() {
                let last: &mut Token = last;
                if last.kind == Kind::Word && last.text.chars().next().map(is_unspaced_script).unwrap_or(false) {
                    let start = last.text.as_ptr() as usize - line.as_ptr() as usize;
                    let end = w.as_ptr() as usize - line.as_ptr() as usize + w.len();
                    last.text = &line[start..end];
                    continue;
                }
            }
        }
        out.push(Token { text: w, kind });
    }
    out
}

/// Scripts written without inter-word spaces, segmented by dictionary lookup.
pub fn is_unspaced_script(c: char) -> bool {
    let u = c as u32;
    matches!(u,
        0x3040..=0x30FF |   // Hiragana, Katakana
        0x3400..=0x4DBF |   // CJK Ext A
        0x4E00..=0x9FFF |   // CJK Unified
        0xF900..=0xFAFF |   // CJK Compatibility
        0x20000..=0x2FA1F | // CJK Ext B+
        0x0E00..=0x0E7F |   // Thai
        0x0E80..=0x0EFF |   // Lao
        0x1780..=0x17FF |   // Khmer
        0x1000..=0x109F |   // Myanmar
        0x0F00..=0x0FFF     // Tibetan
    )
}

pub fn is_hangul(c: char) -> bool {
    matches!(c as u32, 0xAC00..=0xD7A3 | 0x1100..=0x11FF | 0x3130..=0x318F)
}

pub fn is_kana(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30FF)
}

pub fn is_han(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F)
}

/// East Asian punctuation rendered in Latin form (the Elvish output is in Latin script).
pub fn latinize_punct(s: &str) -> Option<String> {
    if !s.chars().any(|c| matches!(c as u32, 0x3000..=0x303F | 0xFF00..=0xFF65)) {
        return None;
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '。' => out.push('.'),
            '、' | '，' => out.push(','),
            '！' => out.push('!'),
            '？' => out.push('?'),
            '：' => out.push(':'),
            '；' => out.push(';'),
            '「' | '『' | '」' | '』' => out.push('"'),
            '（' => out.push('('),
            '）' => out.push(')'),
            '【' | '〔' => out.push('['),
            '】' | '〕' => out.push(']'),
            '《' | '〈' => out.push('«'),
            '》' | '〉' => out.push('»'),
            '・' => out.push('·'),
            '〜' | '～' => out.push('~'),
            '　' => out.push(' '),
            '‥' | '…' => out.push('…'),
            _ => out.push(c),
        }
    }
    Some(out)
}
