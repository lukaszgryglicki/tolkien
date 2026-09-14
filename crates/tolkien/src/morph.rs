//! Suffix-rewrite guesses for inflected forms the lexicon lacks. Each rule maps a word
//! ending to a lemma ending plus the features the ending expresses; the caller keeps
//! only guesses whose lemma actually exists in the lexicon.

use tkdata::features as f;

struct Rule {
    lang: &'static str,
    suffix: &'static str,
    lemma: &'static str,
    tags: u64,
}

const fn r(lang: &'static str, suffix: &'static str, lemma: &'static str, tags: u64) -> Rule {
    Rule { lang, suffix, lemma, tags }
}

const RULES: &[Rule] = &[
    // Serbo-Croatian l-participle (perfect with "je/su") and present tense
    r("sh", "ao", "ati", f::PAST | f::SG | f::MASC),
    r("sh", "ala", "ati", f::PAST | f::SG | f::FEM),
    r("sh", "alo", "ati", f::PAST | f::SG | f::NEUT),
    r("sh", "ali", "ati", f::PAST | f::PL),
    r("sh", "ale", "ati", f::PAST | f::PL),
    r("sh", "io", "iti", f::PAST | f::SG | f::MASC),
    r("sh", "ila", "iti", f::PAST | f::SG | f::FEM),
    r("sh", "ili", "iti", f::PAST | f::PL),
    r("sh", "eo", "eti", f::PAST | f::SG | f::MASC),
    r("sh", "ela", "eti", f::PAST | f::SG | f::FEM),
    r("sh", "eli", "eti", f::PAST | f::PL),
    r("sh", "am", "ati", f::PRES | f::SG | f::P1),
    r("sh", "aš", "ati", f::PRES | f::SG | f::P2),
    r("sh", "amo", "ati", f::PRES | f::PL | f::P1),
    r("sh", "aju", "ati", f::PRES | f::PL | f::P3),
    r("sh", "im", "iti", f::PRES | f::SG | f::P1),
    r("sh", "imo", "iti", f::PRES | f::PL | f::P1),
    // Finnish past tense (3sg -i) and a few common present endings
    r("fi", "oi", "aa", f::PAST | f::SG | f::P3),
    r("fi", "oi", "oa", f::PAST | f::SG | f::P3),
    r("fi", "si", "taa", f::PAST | f::SG | f::P3),
    r("fi", "si", "tää", f::PAST | f::SG | f::P3),
    r("fi", "i", "ä", f::PAST | f::SG | f::P3),
    r("fi", "i", "a", f::PAST | f::SG | f::P3),
    r("fi", "ivat", "a", f::PAST | f::PL | f::P3),
    r("fi", "ivät", "ä", f::PAST | f::PL | f::P3),
    r("fi", "in", "a", f::PAST | f::SG | f::P1),
    r("fi", "vat", "", f::PRES | f::PL | f::P3),
    r("fi", "vät", "", f::PRES | f::PL | f::P3),
    // Turkish past (-di/-dı/-du/-dü + person) and present
    r("tr", "di", "mek", f::PAST | f::SG | f::P3),
    r("tr", "dı", "mak", f::PAST | f::SG | f::P3),
    r("tr", "du", "mak", f::PAST | f::SG | f::P3),
    r("tr", "dü", "mek", f::PAST | f::SG | f::P3),
    r("tr", "ti", "mek", f::PAST | f::SG | f::P3),
    r("tr", "tı", "mak", f::PAST | f::SG | f::P3),
    r("tr", "tu", "mak", f::PAST | f::SG | f::P3),
    r("tr", "tü", "mek", f::PAST | f::SG | f::P3),
    r("tr", "yor", "mek", f::PRES | f::SG | f::P3),
    r("tr", "yor", "mak", f::PRES | f::SG | f::P3),
    r("tr", "ler", "", f::PL),
    r("tr", "lar", "", f::PL),
    // Hungarian past (-t/-tt) and plural
    r("hu", "ott", "", f::PAST | f::SG | f::P3),
    r("hu", "ett", "", f::PAST | f::SG | f::P3),
    r("hu", "ött", "", f::PAST | f::SG | f::P3),
    r("hu", "t", "", f::PAST | f::SG | f::P3),
    r("hu", "ok", "", f::PL),
    r("hu", "ek", "", f::PL),
    r("hu", "ak", "", f::PL),
    r("hu", "ott", "ik", f::PAST | f::SG | f::P3),
];

/// Lemma guesses `(lemma, tags)` for `word` (folded) in `lang`, longest suffix first.
pub fn guesses(lang: &str, word: &str) -> Vec<(String, u64)> {
    match lang {
        "ja" => return ja_guesses(word),
        "ko" => return ko_guesses(word),
        _ => {}
    }
    let mut out: Vec<(String, u64)> = Vec::new();
    let n = word.chars().count();
    for rule in RULES.iter().filter(|r| r.lang == lang) {
        if let Some(stem) = word.strip_suffix(rule.suffix) {
            let sl = stem.chars().count();
            if sl < 1 || n - sl < 1 {
                continue;
            }
            out.push((format!("{}{}", stem, rule.lemma), rule.tags));
        }
    }
    out.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    out
}

// ------------------------------------------------------------------ Japanese

/// Godan verb stems: the vowel-row kana of the masu/nai/te stems and the dictionary ending.
const GODAN_I: &[(char, char)] = &[('い', 'う'), ('き', 'く'), ('ぎ', 'ぐ'), ('し', 'す'), ('ち', 'つ'), ('に', 'ぬ'), ('び', 'ぶ'), ('み', 'む'), ('り', 'る')];
const GODAN_A: &[(char, char)] = &[('わ', 'う'), ('か', 'く'), ('が', 'ぐ'), ('さ', 'す'), ('た', 'つ'), ('な', 'ぬ'), ('ば', 'ぶ'), ('ま', 'む'), ('ら', 'る')];

#[derive(Clone, Copy)]
enum Stem {
    Masu,
    Nai,
    Ta,
    Da,
}

fn push_ja(out: &mut Vec<(String, u64)>, lemma: String, tags: u64) {
    if !lemma.is_empty() && !out.iter().any(|(l, _)| *l == lemma) {
        out.push((lemma, tags));
    }
}

/// Dictionary forms compatible with a conjugation stem (`stem` = word minus the ending).
fn ja_stems(out: &mut Vec<(String, u64)>, stem: &str, kind: Stem, tags: u64) {
    if stem.is_empty() {
        return;
    }
    let last = stem.chars().last().unwrap();
    let body: String = stem.chars().take(stem.chars().count() - 1).collect();
    match kind {
        Stem::Masu | Stem::Nai => {
            // irregular する / 来る
            if last == 'し' {
                push_ja(out, format!("{}する", body), tags);
            }
            if last == '来' || (last == 'き' && body.is_empty()) {
                push_ja(out, format!("{}来る", body), tags);
            }
            let table = if matches!(kind, Stem::Masu) { GODAN_I } else { GODAN_A };
            for &(k, dict) in table {
                if last == k {
                    push_ja(out, format!("{}{}", body, dict), tags);
                }
            }
            // ichidan (与え → 与える, 食べ → 食べる)
            push_ja(out, format!("{}る", stem), tags);
        }
        Stem::Ta => {
            match last {
                'っ' => {
                    if body.ends_with('行') {
                        push_ja(out, format!("{}く", body), tags);
                    }
                    push_ja(out, format!("{}う", body), tags);
                    push_ja(out, format!("{}つ", body), tags);
                    push_ja(out, format!("{}る", body), tags);
                }
                'い' => push_ja(out, format!("{}く", body), tags),
                'し' => {
                    push_ja(out, format!("{}する", body), tags);
                    push_ja(out, format!("{}す", body), tags);
                }
                '来' => push_ja(out, format!("{}来る", body), tags),
                _ => {}
            }
            push_ja(out, format!("{}る", stem), tags);
        }
        Stem::Da => match last {
            'い' => push_ja(out, format!("{}ぐ", body), tags),
            'ん' => {
                push_ja(out, format!("{}む", body), tags);
                push_ja(out, format!("{}ぶ", body), tags);
                push_ja(out, format!("{}ぬ", body), tags);
            }
            _ => {}
        },
    }
}

fn ja_guesses(word: &str) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = Vec::new();
    let past = f::PAST | f::SG | f::P3;
    let pres = f::PRES | f::SG | f::P3;
    // copula and fixed forms
    for (form, lemma, tags) in [
        ("だった", "だ", past),
        ("でした", "です", past | f::FORMAL),
        ("ではない", "だ", pres | f::NEG),
        ("じゃない", "だ", pres | f::NEG),
        ("ではありません", "です", pres | f::NEG),
        ("ではなかった", "だ", past | f::NEG),
        ("じゃなかった", "だ", past | f::NEG),
        ("います", "いる", pres),
        ("いました", "いる", past),
        ("あります", "ある", pres),
        ("ありました", "ある", past),
        ("ありません", "ある", pres | f::NEG),
        ("ない", "ない", pres),
        ("なかった", "ない", past),
        ("きた", "来る", past),
        ("きました", "来る", past),
        ("できた", "できる", past),
        ("できます", "できる", pres),
    ] {
        if word == form {
            push_ja(&mut out, lemma.to_string(), tags);
            return out;
        }
    }
    // verbs: polite, negative, progressive, past, te-form (longest endings first)
    let verb_endings: &[(&str, Stem, u64)] = &[
        ("ませんでした", Stem::Masu, past | f::NEG | f::FORMAL),
        ("ましょう", Stem::Masu, f::FUT | f::PL | f::P1),
        ("ました", Stem::Masu, past | f::FORMAL),
        ("ません", Stem::Masu, pres | f::NEG | f::FORMAL),
        ("ます", Stem::Masu, pres | f::FORMAL),
        ("なかった", Stem::Nai, past | f::NEG),
        ("なければ", Stem::Nai, f::COND | f::NEG),
        ("ない", Stem::Nai, pres | f::NEG),
        ("ていました", Stem::Ta, past | f::IMPF),
        ("ています", Stem::Ta, pres | f::IMPF),
        ("ていた", Stem::Ta, past | f::IMPF),
        ("ている", Stem::Ta, pres | f::IMPF),
        ("てる", Stem::Ta, pres | f::IMPF),
        ("でいました", Stem::Da, past | f::IMPF),
        ("でいます", Stem::Da, pres | f::IMPF),
        ("でいた", Stem::Da, past | f::IMPF),
        ("でいる", Stem::Da, pres | f::IMPF),
        ("たら", Stem::Ta, f::COND),
        ("たり", Stem::Ta, f::PTCP),
        ("た", Stem::Ta, past),
        ("だ", Stem::Da, past),
        ("て", Stem::Ta, f::PTCP),
        ("で", Stem::Da, f::PTCP),
        ("よう", Stem::Masu, f::FUT | f::PL | f::P1),
        ("たかった", Stem::Masu, past | f::OPT),
        ("たくない", Stem::Masu, pres | f::OPT | f::NEG),
        ("たい", Stem::Masu, pres | f::OPT),
        ("れば", Stem::Masu, f::COND),
    ];
    for (suf, kind, tags) in verb_endings {
        if let Some(stem) = word.strip_suffix(suf) {
            ja_stems(&mut out, stem, *kind, *tags);
        }
    }
    // i-adjectives
    for (suf, lemma, tags) in [
        ("くなかった", "い", past | f::NEG),
        ("かった", "い", past),
        ("くない", "い", pres | f::NEG),
        ("ければ", "い", f::COND),
        ("くて", "い", 0),
        ("く", "い", 0),
    ] {
        if let Some(stem) = word.strip_suffix(suf) {
            if !stem.is_empty() {
                push_ja(&mut out, format!("{}{}", stem, lemma), tags);
            }
        }
    }
    // plural suffixes and reduplication (王たち, 人々)
    for suf in ["たち", "達", "ら"] {
        if let Some(stem) = word.strip_suffix(suf) {
            if !stem.is_empty() {
                push_ja(&mut out, stem.to_string(), f::PL);
            }
        }
    }
    if let Some(stem) = word.strip_suffix('々') {
        if !stem.is_empty() {
            push_ja(&mut out, stem.to_string(), f::PL);
        }
    }
    // honorific prefix
    if let Some(stem) = word.strip_prefix('お').or_else(|| word.strip_prefix('御')) {
        if stem.chars().count() >= 2 {
            push_ja(&mut out, stem.to_string(), 0);
        }
    }
    out
}

// ------------------------------------------------------------------ Korean

const HANGUL_BASE: u32 = 0xAC00;

fn hangul_parts(c: char) -> Option<(u32, u32, u32)> {
    let u = c as u32;
    if !(HANGUL_BASE..=0xD7A3).contains(&u) {
        return None;
    }
    let i = u - HANGUL_BASE;
    Some((i / 588, (i % 588) / 28, i % 28))
}

fn hangul_compose(l: u32, v: u32, t: u32) -> char {
    char::from_u32(HANGUL_BASE + l * 588 + v * 28 + t).unwrap_or('\u{fffd}')
}

/// Remove a final consonant (jongseong index) from the last syllable.
fn strip_final(stem: &str, jong: u32) -> Option<String> {
    let last = stem.chars().last()?;
    let (l, v, t) = hangul_parts(last)?;
    if t != jong {
        return None;
    }
    let body: String = stem.chars().take(stem.chars().count() - 1).collect();
    Some(format!("{}{}", body, hangul_compose(l, v, 0)))
}

/// Undo vowel contraction of the past-tense infix (갔 → 가, 왔 → 오, 했 → 하 …).
fn ko_uncontract(stem: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Some(last) = stem.chars().last() else { return out };
    let body: String = stem.chars().take(stem.chars().count() - 1).collect();
    let Some((l, v, t)) = hangul_parts(last) else { return out };
    if t != 20 {
        // ㅆ final: 았/었 contracted into the stem syllable
        return out;
    }
    // vowel indices: ㅏ0 ㅐ1 ㅑ2 ㅒ3 ㅓ4 ㅔ5 ㅕ6 ㅖ7 ㅗ8 ㅘ9 ㅙ10 ㅚ11 ㅛ12 ㅜ13 ㅝ14 ㅞ15 ㅟ16 ㅠ17 ㅡ18 ㅢ19 ㅣ20
    let vowels: &[u32] = match v {
        0 => &[0],       // 갔 → 가
        4 => &[4, 18],   // 섰 → 서, 썼 → 쓰
        9 => &[8],       // 왔 → 오
        14 => &[13],     // 줬 → 주
        6 => &[20],      // 쳤 → 치 / 했 → 하 handled below
        1 => &[1],       // 냈 → 내
        5 => &[5],       // 셌 → 세
        11 => &[11],     // 됐 → 되? (됐 = 되+었) → 되
        16 => &[16],
        _ => &[],
    };
    for &nv in vowels {
        out.push(format!("{}{}", body, hangul_compose(l, nv, 0)));
    }
    if v == 6 && l == 18 {
        // 했 → 하 (하다 verbs)
        out.push(format!("{}{}", body, hangul_compose(l, 0, 0)));
    }
    if v == 11 {
        out.push(format!("{}{}", body, hangul_compose(l, 11, 0)));
    }
    out
}

fn ko_guesses(word: &str) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = Vec::new();
    let past = f::PAST | f::SG | f::P3;
    let pres = f::PRES | f::SG | f::P3;
    let push = |out: &mut Vec<(String, u64)>, lemma: String, tags: u64| {
        if !lemma.is_empty() && !out.iter().any(|(l, _)| *l == lemma) {
            out.push((lemma, tags));
        }
    };
    // (ending, tags, needs past-infix uncontraction)
    let endings: &[(&str, u64, bool)] = &[
        ("었습니다", past | f::FORMAL, false),
        ("았습니다", past | f::FORMAL, false),
        ("였습니다", past | f::FORMAL, false),
        ("습니다", pres | f::FORMAL, false),
        ("ㅂ니다", pres | f::FORMAL, false),
        ("었어요", past | f::FORMAL, false),
        ("았어요", past | f::FORMAL, false),
        ("였어요", past | f::FORMAL, false),
        ("ㅆ어요", past | f::FORMAL, true),
        ("었다", past, false),
        ("았다", past, false),
        ("였다", past, false),
        ("ㅆ다", past, true),
        ("ㅆ습니다", past | f::FORMAL, true),
        ("는다", pres, false),
        ("ㄴ다", pres, false),
        ("어요", pres | f::FORMAL, false),
        ("아요", pres | f::FORMAL, false),
        ("세요", pres | f::FORMAL, false),
        ("었니", past, false),
        ("았니", past, false),
        ("였니", past, false),
        ("니", pres, false),
        ("나요", pres | f::FORMAL, false),
        ("습니까", pres | f::FORMAL, false),
        ("었습니까", past | f::FORMAL, false),
        ("았습니까", past | f::FORMAL, false),
        ("ㅂ니까", pres | f::FORMAL, false),
        ("겠다", f::FUT | f::SG | f::P3, false),
        ("겠습니다", f::FUT | f::FORMAL, false),
        ("고", f::PTCP, false),
        ("며", f::PTCP, false),
        ("면", f::COND, false),
        ("지만", f::PTCP, false),
        ("다", 0, false),
    ];
    for (suf, tags, uncontract) in endings {
        let stem = match *suf {
            "ㅂ니다" | "ㅂ니까" => match word.strip_suffix("니다").or_else(|| word.strip_suffix("니까")) {
                Some(s) => match strip_final(s, 17) {
                    Some(x) => x,
                    None => continue,
                },
                None => continue,
            },
            "ㄴ다" => match word.strip_suffix('다') {
                Some(s) => match strip_final(s, 4) {
                    Some(x) => x,
                    None => continue,
                },
                None => continue,
            },
            "ㅆ어요" | "ㅆ다" | "ㅆ습니다" => {
                let rest = &suf[3..];
                match word.strip_suffix(rest) {
                    Some(s) => s.to_string(),
                    None => continue,
                }
            }
            _ => match word.strip_suffix(suf) {
                Some(s) => s.to_string(),
                None => continue,
            },
        };
        if stem.is_empty() {
            continue;
        }
        if *uncontract {
            for base in ko_uncontract(&stem) {
                push(&mut out, format!("{}다", base), *tags);
            }
        } else {
            push(&mut out, format!("{}다", stem), *tags);
            // 이다 copula attached to nouns: 왕이다 → 왕 + 이다
            if let Some(n) = stem.strip_suffix('이') {
                if !n.is_empty() && *suf == "다" {
                    push(&mut out, n.to_string(), 0);
                }
            }
        }
    }
    // plural suffix
    if let Some(stem) = word.strip_suffix("들") {
        if !stem.is_empty() {
            push(&mut out, stem.to_string(), f::PL);
        }
    }
    out
}
