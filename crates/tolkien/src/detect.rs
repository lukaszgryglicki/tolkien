//! Source-language detection: whatlang statistics combined with dictionary voting.

use crate::data::Data;
use crate::tokenize::{tokenize, Kind};
use tkdata::fold::{fold_en, fold_key};
use tkdata::pos;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Script {
    Latin,
    Cyrillic,
    Greek,
    Arabic,
    Hebrew,
    Devanagari,
    Bengali,
    Gurmukhi,
    Gujarati,
    Tamil,
    Telugu,
    Kannada,
    Malayalam,
    Sinhala,
    Thai,
    Lao,
    Khmer,
    Myanmar,
    Georgian,
    Armenian,
    Ethiopic,
    Hangul,
    Kana,
    Han,
    Tibetan,
    Other,
}

pub fn script_of(c: char) -> Script {
    let u = c as u32;
    match u {
        0x0041..=0x024F | 0x1E00..=0x1EFF => Script::Latin,
        0x0400..=0x052F => Script::Cyrillic,
        0x0370..=0x03FF | 0x1F00..=0x1FFF => Script::Greek,
        0x0600..=0x06FF | 0x0750..=0x077F | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => Script::Arabic,
        0x0590..=0x05FF => Script::Hebrew,
        0x0900..=0x097F => Script::Devanagari,
        0x0980..=0x09FF => Script::Bengali,
        0x0A00..=0x0A7F => Script::Gurmukhi,
        0x0A80..=0x0AFF => Script::Gujarati,
        0x0B80..=0x0BFF => Script::Tamil,
        0x0C00..=0x0C7F => Script::Telugu,
        0x0C80..=0x0CFF => Script::Kannada,
        0x0D00..=0x0D7F => Script::Malayalam,
        0x0D80..=0x0DFF => Script::Sinhala,
        0x0E00..=0x0E7F => Script::Thai,
        0x0E80..=0x0EFF => Script::Lao,
        0x1780..=0x17FF => Script::Khmer,
        0x1000..=0x109F => Script::Myanmar,
        0x0F00..=0x0FFF => Script::Tibetan,
        0x10A0..=0x10FF => Script::Georgian,
        0x0530..=0x058F => Script::Armenian,
        0x1200..=0x139F => Script::Ethiopic,
        0xAC00..=0xD7AF | 0x1100..=0x11FF | 0x3130..=0x318F => Script::Hangul,
        0x3040..=0x30FF | 0x31F0..=0x31FF => Script::Kana,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F => Script::Han,
        _ => Script::Other,
    }
}

/// Dominant script of a text sample.
pub fn dominant_script(text: &str) -> Script {
    let mut counts: Vec<(Script, usize)> = Vec::new();
    for c in text.chars().take(4000) {
        if !c.is_alphabetic() {
            continue;
        }
        let s = script_of(c);
        if s == Script::Other {
            continue;
        }
        match counts.iter_mut().find(|(x, _)| *x == s) {
            Some(e) => e.1 += 1,
            None => counts.push((s, 1)),
        }
    }
    // Japanese: any kana means Japanese even if Han dominates
    if counts.iter().any(|(s, n)| *s == Script::Kana && *n > 0) {
        let han = counts.iter().find(|(s, _)| *s == Script::Han).map(|x| x.1).unwrap_or(0);
        let kana = counts.iter().find(|(s, _)| *s == Script::Kana).map(|x| x.1).unwrap_or(0);
        let latin = counts.iter().find(|(s, _)| *s == Script::Latin).map(|x| x.1).unwrap_or(0);
        if kana + han > latin {
            return Script::Kana;
        }
    }
    counts.into_iter().max_by_key(|(_, n)| *n).map(|(s, _)| s).unwrap_or(Script::Other)
}

/// Candidate languages per script (Wiktionary codes), roughly by prevalence.
pub fn candidates(script: Script) -> &'static [&'static str] {
    match script {
        Script::Latin => &[
            "en", "es", "fr", "de", "pt", "it", "nl", "pl", "sv", "da", "nb", "nn", "fi", "et", "hu", "cs", "sk", "sl",
            "sh", "ro", "lt", "lv", "tr", "az", "id", "ms", "tl", "vi", "eo", "la", "ca", "gl", "eu", "cy", "ga", "gd",
            "br", "is", "fo", "af", "sw", "mt", "sq", "lb", "ceb", "haw", "sm", "mi", "to", "fj", "ht", "yo", "ig", "ha",
            "zu", "xh", "st", "so", "uz", "tk", "ku", "kmr", "qu", "ay", "gn", "jv", "su", "hil", "war", "ilo", "wa",
            "oc", "sc", "co", "rm", "fy", "li", "nds", "gsw", "bar", "szl", "csb", "hsb", "dsb", "vo", "io", "ia", "jbo",
        ],
        Script::Cyrillic => &["ru", "uk", "be", "bg", "mk", "sh", "kk", "ky", "mn", "tg", "tt", "ba", "cv", "os", "ce", "sah", "kv", "udm", "myv", "mhr", "ab", "kbd", "av", "ug", "rue", "cu", "orv"],
        Script::Greek => &["el", "grc"],
        Script::Arabic => &["ar", "fa", "ur", "ps", "ug", "ckb", "sd", "ms", "ks", "pnb", "arz", "ary", "apc", "acw", "afb", "ota"],
        Script::Hebrew => &["he", "yi", "arc", "lad"],
        Script::Devanagari => &["hi", "mr", "ne", "sa", "bh", "mag", "new", "kok", "awa", "doi"],
        Script::Bengali => &["bn", "as", "syl"],
        Script::Gurmukhi => &["pa"],
        Script::Gujarati => &["gu"],
        Script::Tamil => &["ta"],
        Script::Telugu => &["te"],
        Script::Kannada => &["kn"],
        Script::Malayalam => &["ml"],
        Script::Sinhala => &["si"],
        Script::Thai => &["th"],
        Script::Lao => &["lo"],
        Script::Khmer => &["km"],
        Script::Myanmar => &["my", "shn", "mnw"],
        Script::Georgian => &["ka", "xmf"],
        Script::Armenian => &["hy", "xcl"],
        Script::Ethiopic => &["am", "ti", "gez"],
        Script::Hangul => &["ko"],
        Script::Kana => &["ja"],
        Script::Han => &["zh", "ja", "yue", "lzh"],
        Script::Tibetan => &["bo", "dz"],
        Script::Other => &["en"],
    }
}

/// Map a whatlang language to a Wiktionary language code.
pub fn whatlang_code(l: whatlang::Lang) -> &'static str {
    use whatlang::Lang::*;
    match l {
        Epo => "eo",
        Eng => "en",
        Rus => "ru",
        Cmn => "zh",
        Spa => "es",
        Por => "pt",
        Ita => "it",
        Ben => "bn",
        Fra => "fr",
        Deu => "de",
        Ukr => "uk",
        Kat => "ka",
        Ara => "ar",
        Hin => "hi",
        Jpn => "ja",
        Heb => "he",
        Yid => "yi",
        Pol => "pl",
        Amh => "am",
        Jav => "jv",
        Kor => "ko",
        Nob => "nb",
        Dan => "da",
        Swe => "sv",
        Fin => "fi",
        Tur => "tr",
        Nld => "nl",
        Hun => "hu",
        Ces => "cs",
        Ell => "el",
        Bul => "bg",
        Bel => "be",
        Mar => "mr",
        Kan => "kn",
        Ron => "ro",
        Slv => "sl",
        Hrv => "sh",
        Srp => "sh",
        Mkd => "mk",
        Lit => "lt",
        Lav => "lv",
        Est => "et",
        Tam => "ta",
        Vie => "vi",
        Urd => "ur",
        Tha => "th",
        Guj => "gu",
        Uzb => "uz",
        Pan => "pa",
        Aze => "az",
        Ind => "id",
        Tel => "te",
        Pes => "fa",
        Mal => "ml",
        Ori => "or",
        Mya => "my",
        Nep => "ne",
        Sin => "si",
        Khm => "km",
        Tuk => "tk",
        Aka => "ak",
        Zul => "zu",
        Sna => "sn",
        Afr => "af",
        Lat => "la",
        Slk => "sk",
        Cat => "ca",
        Tgl => "tl",
        Hye => "hy",
    }
}

#[derive(Clone, Debug)]
pub struct Detection {
    pub lang: String,
    /// Fraction of words found in the dictionary of the chosen language.
    pub hit_rate: f32,
    pub words: usize,
}

/// Does the (folded) word exist in the dictionary of `lang`?
pub fn known(data: &Data, lang: &str, word: &str) -> bool {
    if lang == "en" {
        data.en_fst.contains_key(fold_en(word).as_bytes())
    } else {
        data.lex_has(lang, &fold_key(lang, word))
    }
}

/// Evidence weight of a dictionary hit: function words (articles, pronouns, prepositions,
/// conjunctions) are strong evidence for a language, content words weak (Wiktionary's English
/// lexicon lists many foreign names and loans).
fn hit_weight(data: &Data, lang: &str, word: &str) -> Option<f32> {
    let key = format!("{}\u{1}{}", lang, word);
    if let Some(v) = data.hit_cache.borrow().get(&key) {
        return *v;
    }
    let v = hit_weight_uncached(data, lang, word);
    let mut cache = data.hit_cache.borrow_mut();
    if cache.len() >= 65536 {
        cache.clear();
    }
    cache.insert(key, v);
    v
}

fn hit_weight_uncached(data: &Data, lang: &str, word: &str) -> Option<f32> {
    if lang == "en" {
        let f = fold_en(word);
        if !data.en_fst.contains_key(f.as_bytes()) {
            return None;
        }
        return Some(if crate::english::role(&f).is_some() { 1.0 } else { 0.6 });
    }
    let f = fold_key(lang, word);
    if !data.lex_has(lang, &f) {
        return None;
    }
    let func = data
        .lex_lookup(lang, &f)
        .map(|cs| cs.iter().any(|c| matches!(c.pos, pos::PRON | pos::DET | pos::PREP | pos::CONJ | pos::PART)))
        .unwrap_or(false);
    Some(if func { 1.0 } else { 0.6 })
}

/// Detect the language of `text`. `hint` biases towards a previously detected language.
pub fn detect(data: &Data, text: &str, hint: Option<&str>) -> Detection {
    detect_ex(data, text, hint, false)
}

/// `quick`: consider only the major languages of the script (plus whatlang's guess and the
/// hint) — for re-detecting single lines inside a document.
pub fn detect_ex(data: &Data, text: &str, hint: Option<&str>, quick: bool) -> Detection {
    let script = dominant_script(text);
    let all = candidates(script);
    let mut cands: Vec<&str> = if quick { all[..all.len().min(24)].to_vec() } else { all.to_vec() };
    let wl = whatlang::detect(text);
    let (wl_code, wl_conf) = match wl {
        Some(info) => (Some(whatlang_code(info.lang())), info.confidence() as f32),
        None => (None, 0.0),
    };
    if std::env::var_os("TOLKIEN_DEBUG").is_some() {
        eprintln!("whatlang {:?} conf={:.3}", wl_code, wl_conf);
    }
    if let Some(c) = wl_code {
        if !cands.contains(&c) {
            cands.push(c);
        }
    }
    if let Some(h) = hint {
        if !cands.contains(&h) {
            cands.push(h);
        }
    }
    // sample words
    let toks = tokenize(text);
    let words: Vec<&str> = toks
        .iter()
        .filter(|t| t.kind == Kind::Word)
        .map(|t| t.text)
        .filter(|w| w.chars().count() >= 2 || script != Script::Latin || matches!(*w, "I" | "a"))
        .take(400)
        .collect();
    if words.is_empty() {
        let lang = hint.map(|s| s.to_string()).or(wl_code.map(|s| s.to_string())).unwrap_or_else(|| "en".to_string());
        return Detection { lang, hit_rate: 0.0, words: 0 };
    }
    let unspaced = matches!(script, Script::Han | Script::Kana | Script::Thai | Script::Lao | Script::Khmer | Script::Myanmar | Script::Tibetan);
    let mut best: Option<(&str, f32, f32)> = None;
    // whatlang's trigram statistics are unreliable on a handful of words
    let wl_flat = 0.15 * (words.len() as f32 / 8.0).min(1.0);
    for lang in &cands {
        let mut hits = 0usize;
        let mut weighted = 0.0f32;
        let mut total = 0usize;
        for w in &words {
            total += 1;
            if unspaced {
                // count a hit if any prefix of the run is a dictionary word
                let f = fold_key(lang, w);
                let mut found = false;
                let mut end_positions: Vec<usize> = f.char_indices().map(|(i, _)| i).skip(1).collect();
                end_positions.push(f.len());
                for e in end_positions.iter().take(6) {
                    if data.lex_has(lang, &f[..*e]) {
                        found = true;
                        break;
                    }
                }
                if found {
                    hits += 1;
                    weighted += 1.0;
                }
            } else if let Some(wt) = hit_weight(data, lang, w) {
                hits += 1;
                weighted += wt;
            }
        }
        if total == 0 {
            continue;
        }
        let rate = hits as f32 / total as f32;
        let mut score = weighted / total as f32;
        if Some(*lang) == wl_code {
            // a near-zero confidence must not break dictionary ties
            score += (wl_flat + 0.25 * wl_conf) * (wl_conf / 0.1).min(1.0);
        }
        if Some(*lang) == hint {
            score += 0.10;
        }
        // prior: prefer languages earlier in the candidate list on near ties
        let idx = cands.iter().position(|c| c == lang).unwrap_or(0) as f32;
        score -= idx * 0.002;
        if best.map(|b| score > b.1).unwrap_or(true) {
            best = Some((lang, score, rate));
        }
    }
    let (lang, _, rate) = best.unwrap_or(("en", 0.0, 0.0));
    Detection { lang: lang.to_string(), hit_rate: rate, words: words.len() }
}
