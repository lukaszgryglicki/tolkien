//! Quenya inflection engine (nouns, adjectives, verbs, pronouns, numerals),
//! following Tolkien's late Quenya as systematised by Eldamo's Neo-Quenya course.

use crate::data::Entry;
use tkdata::vclass;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)] // dual and partitive plural are declined but not yet selected by the parser
pub enum Number {
    Sg,
    Pl,
    Du,
    Part,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Case {
    Nom,
    Gen,
    Poss,
    Dat,
    All,
    Abl,
    Loc,
    Ins,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)] // the full pronominal paradigm is modelled; dual/inclusive persons are not yet produced
pub enum Person {
    S1,
    S2Fam,
    S2Pol,
    S3,
    S3Inan,
    P1Ex,
    P1In,
    P2,
    P3,
    P3Inan,
    D1Ex,
    D1In,
    D2,
    D3,
}

impl Person {
    pub fn is_plural(self) -> bool {
        matches!(self, Person::P1Ex | Person::P1In | Person::P2 | Person::P3 | Person::P3Inan)
    }
    pub fn is_dual(self) -> bool {
        matches!(self, Person::D1Ex | Person::D1In | Person::D2 | Person::D3)
    }
    /// Independent (object) pronoun.
    pub fn independent(self) -> &'static str {
        match self {
            Person::S1 => "ni",
            Person::S2Fam => "tye",
            Person::S2Pol => "lye",
            Person::S3 => "se",
            Person::S3Inan => "sa",
            Person::P1Ex => "me",
            Person::P1In => "ve",
            Person::P2 => "le",
            Person::P3 => "te",
            Person::P3Inan => "tai",
            Person::D1Ex => "met",
            Person::D1In => "vet",
            Person::D2 => "let",
            Person::D3 => "tu",
        }
    }
    /// Emphatic (subject) pronoun.
    pub fn emphatic(self) -> &'static str {
        match self {
            Person::S1 => "inyë",
            Person::S2Fam => "etyë",
            Person::S2Pol => "elyë",
            Person::S3 => "issë",
            Person::S3Inan => "issa",
            Person::P1Ex => "elmë",
            Person::P1In => "elvë",
            Person::P2 => "eldë",
            Person::P3 => "intë",
            Person::P3Inan => "inta",
            Person::D1Ex => "emmë",
            Person::D1In => "inquë",
            Person::D2 => "estë",
            Person::D3 => "ettë",
        }
    }
    /// Dative pronoun ("to me" etc.).
    pub fn dative(self) -> String {
        match self {
            Person::S1 => "nin".to_string(),
            Person::S2Fam => "tyen".to_string(),
            Person::S2Pol => "lyen".to_string(),
            Person::S3 => "sen".to_string(),
            Person::S3Inan => "san".to_string(),
            Person::P1Ex => "men".to_string(),
            Person::P1In => "ven".to_string(),
            Person::P2 => "len".to_string(),
            Person::P3 => "ten".to_string(),
            Person::P3Inan => "tain".to_string(),
            p => format!("{}n", p.independent()),
        }
    }
    /// Reflexive pronoun.
    pub fn reflexive(self) -> &'static str {
        match self {
            Person::S1 => "imni",
            Person::S2Fam => "imtye",
            Person::S2Pol => "imlë",
            Person::S3 => "insë",
            Person::S3Inan => "insa",
            Person::P1Ex => "immë",
            Person::P1In => "imvë",
            Person::P2 => "imlë",
            Person::P3 | Person::P3Inan => "intë",
            _ => "intë",
        }
    }
    /// Subject suffix: (short, long).
    pub fn subject_suffix(self) -> (&'static str, &'static str) {
        match self {
            Person::S1 => ("n", "nyë"),
            Person::S2Fam => ("t", "tyë"),
            Person::S2Pol => ("l", "lyë"),
            Person::S3 => ("s", "ssë"),
            Person::S3Inan => ("s", "ssa"),
            Person::P1Ex => ("lmë", "lmë"),
            Person::P1In => ("lvë", "lvë"),
            Person::P2 => ("ldë", "ldë"),
            Person::P3 => ("ntë", "ntë"),
            Person::P3Inan => ("nta", "nta"),
            Person::D1Ex => ("mmë", "mmë"),
            Person::D1In => ("nquë", "nquë"),
            Person::D2 => ("stë", "stë"),
            Person::D3 => ("ttë", "ttë"),
        }
    }
    /// Possessive suffix with its connecting vowel after consonants.
    pub fn possessive_suffix(self) -> (&'static str, &'static str) {
        match self {
            Person::S1 => ("i", "nya"),
            Person::S2Fam => ("e", "tya"),
            Person::S2Pol => ("e", "lya"),
            Person::S3 | Person::S3Inan => ("", "rya"),
            Person::P1Ex => ("e", "lma"),
            Person::P1In => ("e", "lva"),
            Person::P2 => ("e", "lda"),
            Person::P3 | Person::P3Inan => ("i", "nta"),
            Person::D1Ex => ("e", "mma"),
            Person::D1In => ("i", "nqua"),
            Person::D2 => ("e", "sta"),
            Person::D3 => ("e", "tta"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tense {
    Aorist,
    Present,
    Past,
    Perfect,
    Future,
    PastPerfect,
    PastImperfect,
    FutureImperfect,
    FuturePerfect,
}

// ------------------------------------------------------------------ phonology

pub fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'á' | 'é' | 'í' | 'ó' | 'ú' | 'ä' | 'ë' | 'ï' | 'ö' | 'ü' | 'â' | 'ê' | 'î' | 'ô' | 'û')
}

pub fn is_long_vowel(c: char) -> bool {
    matches!(c, 'á' | 'é' | 'í' | 'ó' | 'ú' | 'â' | 'ê' | 'î' | 'ô' | 'û')
}

pub fn plain_vowel(c: char) -> char {
    match c {
        'á' | 'ä' | 'â' => 'a',
        'é' | 'ë' | 'ê' => 'e',
        'í' | 'ï' | 'î' => 'i',
        'ó' | 'ö' | 'ô' => 'o',
        'ú' | 'ü' | 'û' => 'u',
        c => c,
    }
}

pub fn lengthen(c: char) -> char {
    match plain_vowel(c) {
        'a' => 'á',
        'e' => 'é',
        'i' => 'í',
        'o' => 'ó',
        'u' => 'ú',
        c => c,
    }
}

/// Remove diaereses (they are purely orthographic in Quenya).
pub fn strip_diaeresis(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'ë' => 'e',
            'ä' => 'a',
            'ö' => 'o',
            'ï' => 'i',
            'ü' => 'u',
            'Ë' => 'E',
            'Ä' => 'A',
            'Ö' => 'O',
            c => c,
        })
        .collect()
}

/// Re-apply Tolkien's diaeresis conventions to a plain-spelled word.
pub fn respell(s: &str) -> String {
    let plain = strip_diaeresis(s);
    let chars: Vec<char> = plain.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(plain.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };
        let next = chars.get(i + 1).copied();
        match c {
            'e' if next.is_none() && n > 1 && !matches!(prev, Some('i') | Some('e')) => out.push('ë'),
            'e' if next == Some('a') && !matches!(prev, Some(p) if is_vowel(p)) => out.push('ë'),
            'e' if next == Some('o') && !matches!(prev, Some(p) if is_vowel(p)) => out.push('ë'),
            'o' if next == Some('a') && !matches!(prev, Some(p) if is_vowel(p)) => out.push('ö'),
            'e' if next.is_none() && prev == Some('i') => out.push('ë'),
            _ => out.push(c),
        }
    }
    out
}

fn last_char(s: &str) -> Option<char> {
    s.chars().last()
}

fn ends_with_vowel(s: &str) -> bool {
    last_char(s).map(is_vowel).unwrap_or(false)
}

fn strip_last(s: &str) -> &str {
    match s.char_indices().last() {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// Index (char) of the base vowel of a verb stem: the last vowel of a basic
/// stem, or the vowel before the final a/u of a derived stem.
fn base_vowel_index(stem: &str) -> Option<usize> {
    let cs: Vec<char> = stem.chars().collect();
    let n = cs.len();
    if n == 0 {
        return None;
    }
    let mut end = n;
    if is_vowel(cs[n - 1]) {
        end = n - 1;
        // 'ya'/'ta' etc.: skip back to the vowel before
    }
    for i in (0..end).rev() {
        if is_vowel(cs[i]) {
            return Some(i);
        }
    }
    None
}

/// Lengthen the base vowel if phonologically possible.
fn lengthen_base(stem: &str) -> String {
    let cs: Vec<char> = stem.chars().collect();
    let Some(i) = base_vowel_index(stem) else { return stem.to_string() };
    if is_long_vowel(cs[i]) {
        return stem.to_string();
    }
    // diphthong?
    if i + 1 < cs.len() && is_vowel(cs[i + 1]) && !(i + 2 == cs.len()) {
        return stem.to_string();
    }
    if i > 0 && is_vowel(cs[i - 1]) {
        return stem.to_string();
    }
    // consonant cluster after the vowel? (qu counts as one, also ny/ty/ly/ry/hy/hw/nw? treat 'y'/'w' clusters as clusters except qu)
    let mut j = i + 1;
    let mut cons = 0;
    while j < cs.len() && !is_vowel(cs[j]) {
        if cs[j] == 'u' && j > 0 && cs[j - 1] == 'q' {
            j += 1;
            continue;
        }
        cons += 1;
        j += 1;
    }
    if cons > 1 {
        return stem.to_string();
    }
    let mut out: String = cs[..i].iter().collect();
    out.push(lengthen(cs[i]));
    out.extend(cs[i + 1..].iter());
    out
}

fn base_vowel(stem: &str) -> Option<char> {
    let cs: Vec<char> = stem.chars().collect();
    base_vowel_index(stem).map(|i| plain_vowel(cs[i]))
}

// ------------------------------------------------------------------ nouns

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum NClass {
    Vocalic,
    ENoun,
    Consonantal,
}

fn noun_stem(e_form: &str, e_stem: &str) -> (String, String) {
    let form = strip_diaeresis(e_form.trim_end_matches('-'));
    let stem = strip_diaeresis(e_stem.trim_end_matches('-'));
    let stem = if stem.is_empty() { form.clone() } else { stem };
    (form, stem)
}

fn nclass(base: &str) -> NClass {
    match last_char(base) {
        Some('e') | Some('é') => NClass::ENoun,
        Some(c) if is_vowel(c) => NClass::Vocalic,
        _ => NClass::Consonantal,
    }
}

/// Fix consonant+w sequences produced by u-stem plurals (rancu- → ranqui).
fn fix_clusters(s: &str) -> String {
    s.replace("cw", "qu").replace("ngw", "ngw")
}

fn plural_nom(form: &str, stem: &str) -> String {
    let class = nclass(form);
    match class {
        NClass::Vocalic => {
            if form.ends_with('o') && stem.ends_with('u') && stem != form {
                fix_clusters(&format!("{}wi", strip_last(stem)))
            } else {
                format!("{}r", form)
            }
        }
        NClass::ENoun => {
            if stem != form && stem.ends_with('i') {
                stem.to_string()
            } else if form.ends_with("ie") || form.ends_with("le") && form.chars().count() > 3 {
                format!("{}r", form)
            } else {
                format!("{}i", strip_last(form))
            }
        }
        NClass::Consonantal => format!("{}i", stem),
    }
}

fn uses_u_dual(form: &str, stem: &str) -> bool {
    if nclass(form) == NClass::Consonantal {
        return true;
    }
    let cs: Vec<char> = stem.chars().collect();
    let n = cs.len();
    n >= 2 && matches!(cs[n - 2], 't' | 'd')
}

/// Inflect a noun given its dictionary form and stem.
pub fn inflect_noun(form_in: &str, stem_in: &str, number: Number, case: Case, poss: Option<Person>) -> String {
    let (form, stem) = noun_stem(form_in, stem_in);
    let mut base_form = form.clone();
    let mut base_stem = stem.clone();
    if let Some(p) = poss {
        let (join, suf) = p.possessive_suffix();
        let b = if ends_with_vowel(&stem) {
            format!("{}{}", stem, suf)
        } else if suf == "rya" {
            format!("{}ya", stem)
        } else {
            format!("{}{}{}", stem, join, suf)
        };
        base_form = b.clone();
        base_stem = b;
    }
    let form = &base_form;
    let stem = &base_stem;
    let class = nclass(form);
    let out = match number {
        Number::Sg => match class {
            NClass::Vocalic => match case {
                Case::Nom => form.clone(),
                Case::Gen => {
                    if form.ends_with('a') {
                        format!("{}o", strip_last(form))
                    } else if form.ends_with('o') {
                        form.clone()
                    } else {
                        format!("{}o", form)
                    }
                }
                Case::Poss => format!("{}va", form),
                Case::Dat => format!("{}n", form),
                Case::All => format!("{}nna", form),
                Case::Abl => format!("{}llo", form),
                Case::Loc => format!("{}sse", form),
                Case::Ins => format!("{}nen", form),
            },
            NClass::ENoun => match case {
                Case::Nom => form.clone(),
                Case::Gen => format!("{}o", form),
                Case::Poss => format!("{}va", form),
                Case::Dat => format!("{}n", form),
                Case::All => format!("{}nna", form),
                Case::Abl => format!("{}llo", form),
                Case::Loc => format!("{}sse", form),
                Case::Ins => format!("{}nen", form),
            },
            NClass::Consonantal => {
                let last = last_char(stem).unwrap_or('n');
                match case {
                    Case::Nom => form.clone(),
                    Case::Gen => format!("{}o", stem),
                    Case::Poss => format!("{}wa", stem),
                    Case::Dat => format!("{}en", stem),
                    Case::All => match last {
                        'n' => format!("{}na", stem),
                        'l' => format!("{}da", stem),
                        _ => format!("{}enna", stem),
                    },
                    Case::Abl => match last {
                        'l' => format!("{}lo", stem),
                        _ => format!("{}ello", stem),
                    },
                    Case::Loc => match last {
                        's' => format!("{}se", stem),
                        _ => format!("{}esse", stem),
                    },
                    Case::Ins => match last {
                        'r' | 'l' => format!("{}men", stem),
                        'n' | 't' | 'c' => format!("{}wen", stem),
                        _ => format!("{}enen", stem),
                    },
                }
            }
        },
        Number::Pl => {
            let pl = plural_nom(form, stem);
            match class {
                NClass::Vocalic => match case {
                    Case::Nom => pl,
                    Case::Gen => format!("{}on", pl),
                    Case::Poss => format!("{}iva", form),
                    Case::Dat => format!("{}in", form),
                    Case::All => format!("{}nnar", form),
                    Case::Abl => format!("{}llon", form),
                    Case::Loc => format!("{}ssen", form),
                    Case::Ins => format!("{}inen", form),
                },
                NClass::ENoun => {
                    let cut = strip_last(form);
                    match case {
                        Case::Nom => pl,
                        Case::Gen => format!("{}on", pl),
                        Case::Poss => format!("{}íva", cut),
                        Case::Dat => format!("{}in", cut),
                        Case::All => format!("{}nnar", form),
                        Case::Abl => format!("{}llon", form),
                        Case::Loc => format!("{}ssen", form),
                        Case::Ins => format!("{}ínen", cut),
                    }
                }
                NClass::Consonantal => match case {
                    Case::Nom => pl,
                    Case::Gen => format!("{}ion", stem),
                    Case::Poss => format!("{}íva", stem),
                    Case::Dat => format!("{}in", stem),
                    Case::All => format!("{}innar", stem),
                    Case::Abl => format!("{}illon", stem),
                    Case::Loc => format!("{}issen", stem),
                    Case::Ins => format!("{}ínen", stem),
                },
            }
        }
        Number::Du => {
            if uses_u_dual(form, stem) {
                let b = if class == NClass::Consonantal { stem.clone() } else { strip_last(form).to_string() };
                match case {
                    Case::Nom => format!("{}u", b),
                    Case::Gen => format!("{}uo", b),
                    Case::Poss => format!("{}úva", b),
                    Case::Dat => format!("{}un", b),
                    Case::All => format!("{}unna", b),
                    Case::Abl => format!("{}ullo", b),
                    Case::Loc => format!("{}usse", b),
                    Case::Ins => format!("{}únen", b),
                }
            } else {
                match case {
                    Case::Nom => format!("{}t", form),
                    Case::Gen => format!("{}to", form),
                    Case::Poss => format!("{}twa", form),
                    Case::Dat => format!("{}nt", form),
                    Case::All => format!("{}nta", form),
                    Case::Abl => format!("{}lto", form),
                    Case::Loc => format!("{}tse", form),
                    Case::Ins => format!("{}nten", form),
                }
            }
        }
        Number::Part => {
            let pstem = match class {
                NClass::Consonantal => match last_char(stem) {
                    Some('l') => format!("{}", stem),
                    Some('n') | Some('r') => format!("{}l", strip_last(stem)),
                    _ => format!("{}e", stem),
                },
                _ => form.clone(),
            };
            match case {
                Case::Nom => format!("{}li", pstem),
                Case::Gen => format!("{}lion", pstem),
                Case::Poss => format!("{}líva", pstem),
                Case::Dat => format!("{}lin", pstem),
                Case::All => format!("{}linnar", pstem),
                Case::Abl => format!("{}lillon", pstem),
                Case::Loc => format!("{}lissen", pstem),
                Case::Ins => format!("{}línen", pstem),
            }
        }
    };
    respell(&out)
}

/// Inflect a noun entry, preferring attested forms for the plain plural.
pub fn noun(e: &Entry, number: Number, case: Case, poss: Option<Person>) -> String {
    if poss.is_none() {
        let kind = match (number, case) {
            (Number::Sg, Case::Nom) => Some("nominative"),
            (Number::Pl, Case::Nom) => Some("plural"),
            (Number::Pl, Case::Gen) => Some("genitive plural"),
            (Number::Sg, Case::Gen) => Some("genitive"),
            (Number::Sg, Case::Dat) => Some("dative"),
            (Number::Sg, Case::All) => Some("allative"),
            (Number::Sg, Case::Abl) => Some("ablative"),
            (Number::Sg, Case::Loc) => Some("locative"),
            (Number::Sg, Case::Ins) => Some("instrumental"),
            (Number::Sg, Case::Poss) => Some("possessive"),
            (Number::Du, Case::Nom) => Some("dual"),
            (Number::Part, Case::Nom) => Some("partitive-plural"),
            _ => None,
        };
        if let Some(k) = kind {
            if let Some(a) = e.attested(k).filter(|a| is_clean_form(a)) {
                if (number, case) == (Number::Sg, Case::Nom) {
                    return respell(&strip_diaeresis(e.form.trim_end_matches('-')));
                }
                return respell(&strip_diaeresis(a));
            }
        }
        if (number, case) == (Number::Sg, Case::Nom) {
            return respell(&strip_diaeresis(e.form.trim_end_matches('-')));
        }
    }
    inflect_noun(e.form, e.stem, number, case, poss)
}

fn is_clean_form(a: &str) -> bool {
    !a.is_empty()
        && a.chars().all(|c| c.is_alphabetic() || c == '-')
        && a.chars().next().map(|c| c.is_lowercase()).unwrap_or(false)
        && !a.contains('-')
        && !a.chars().any(|c| matches!(c, 'ð' | 'þ' | 'ñ' | 'ŕ' | 'ì' | 'ù' | 'ă' | 'ĭ' | 'ŭ' | 'ŏ' | 'ĕ'))
}

// ------------------------------------------------------------------ adjectives

pub fn adjective_plural(adj: &str) -> String {
    let a = strip_diaeresis(adj.trim_end_matches('-'));
    let out = if a.ends_with("ea") {
        format!("{}ie", &a[..a.len() - 2])
    } else if a.ends_with('a') {
        format!("{}e", strip_last(&a))
    } else if a.ends_with('e') {
        format!("{}i", strip_last(&a))
    } else if a.ends_with('o') {
        format!("{}wi", strip_last(&a))
    } else if ends_with_vowel(&a) {
        a
    } else {
        format!("{}i", a)
    };
    respell(&out)
}

pub fn adjective(e: &Entry, number: Number) -> String {
    let form = e.form.trim_end_matches('-');
    match number {
        Number::Sg | Number::Du => respell(&strip_diaeresis(form)),
        Number::Pl | Number::Part => {
            if let Some(a) = e.attested("plural").filter(|a| is_clean_form(a)) {
                respell(&strip_diaeresis(a))
            } else {
                adjective_plural(form)
            }
        }
    }
}

/// Intensive an- ("very"); `superlative` uses ar-.
pub fn intensify(adj: &str, superlative: bool) -> String {
    let a = strip_diaeresis(adj);
    if superlative {
        return respell(&format!("ar{}", a));
    }
    let first = a.chars().next().unwrap_or('a');
    let prefix = match first {
        'm' | 'p' => "am",
        'l' | 'r' => "ana",
        'h' if a.starts_with("hl") || a.starts_with("hr") => "ana",
        _ => "an",
    };
    respell(&format!("{}{}", prefix, a))
}

pub fn adverb(adj: &str) -> String {
    let a = strip_diaeresis(adj.trim_end_matches('-'));
    let out = if a.ends_with('a') {
        format!("{}ve", a)
    } else if a.ends_with('e') {
        format!("{}ive", strip_last(&a))
    } else if ends_with_vowel(&a) {
        format!("{}ve", a)
    } else {
        format!("{}eve", a)
    };
    respell(&out)
}

// ------------------------------------------------------------------ verbs

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum VClass {
    Basic,
    AStem,
    UStem,
}

fn verb_stem(e: &Entry) -> String {
    strip_diaeresis(e.form.trim_end_matches('-'))
}

fn vclass_of(stem: &str) -> VClass {
    match last_char(stem) {
        Some('a') => VClass::AStem,
        Some('u') => VClass::UStem,
        _ => VClass::Basic,
    }
}

/// Uninflected aorist (also the simple infinitive).
pub fn aorist(e: &Entry) -> String {
    if let Some(a) = e.attested("aorist").or_else(|| e.attested("infinitive")).filter(|a| is_clean_form(a)) {
        return respell(&strip_diaeresis(a));
    }
    let s = verb_stem(e);
    match vclass_of(&s) {
        VClass::Basic => respell(&format!("{}e", s)),
        _ => respell(&s),
    }
}

/// Aorist stem used before pronominal suffixes.
fn aorist_suffix_stem(e: &Entry) -> String {
    let s = verb_stem(e);
    match vclass_of(&s) {
        VClass::Basic => format!("{}i", s),
        _ => s,
    }
}

pub fn present(e: &Entry) -> String {
    if let Some(a) = e.attested("present").filter(|a| is_clean_form(a)) {
        return respell(&strip_diaeresis(a));
    }
    let s = verb_stem(e);
    let out = match vclass_of(&s) {
        VClass::Basic => format!("{}a", lengthen_base(&s)),
        VClass::UStem => format!("{}a", lengthen_base(&s)),
        VClass::AStem => {
            let l = lengthen_base(&s);
            format!("{}ea", strip_last(&l))
        }
    };
    respell(&out)
}

pub fn future(e: &Entry) -> String {
    if let Some(a) = e.attested("future").filter(|a| is_clean_form(a)) {
        return respell(&strip_diaeresis(a));
    }
    let s = verb_stem(e);
    let out = match vclass_of(&s) {
        VClass::Basic => {
            if s.ends_with("uv") {
                format!("{}ua", strip_last(&s))
            } else if s.ends_with('v') {
                format!("{}uva", strip_last(&s))
            } else {
                format!("{}uva", s)
            }
        }
        VClass::AStem => {
            if s.ends_with("wa") {
                format!("{}uva", &s[..s.len() - 2])
            } else {
                format!("{}uva", strip_last(&s))
            }
        }
        VClass::UStem => format!("{}úva", strip_last(&s)),
    };
    respell(&out)
}

fn strong_past(s: &str) -> Option<String> {
    let cs: Vec<char> = s.chars().collect();
    let n = cs.len();
    if n < 2 {
        return None;
    }
    let last = cs[n - 1];
    let head: String = cs[..n - 1].iter().collect();
    let out = match last {
        'p' => format!("{}mpe", head),
        't' => format!("{}nte", head),
        'c' => format!("{}nce", head),
        'f' => format!("{}ppe", head),
        'h' => format!("{}cce", head),
        's' => format!("{}nse", head),
        'r' => format!("{}rne", head),
        'l' => format!("{}lle", head),
        'm' => format!("{}mne", head),
        'n' => format!("{}nne", head),
        'v' => {
            // long-vowel past from the perfect: lav- → lávë
            format!("{}e", lengthen_base(s))
        }
        'u' if n >= 2 && cs[n - 2] == 'q' => format!("{}nque", cs[..n - 2].iter().collect::<String>()),
        'w' | 'y' => format!("{}ne", s),
        _ => return None,
    };
    Some(out)
}

pub fn past(e: &Entry) -> String {
    // (a "past" ending in -ië is a misfiled perfect, e.g. marnië for mar-)
    if let Some(a) = e.attested("past").filter(|a| is_clean_form(a) && !a.ends_with("ië") && !a.ends_with("ie")) {
        return respell(&strip_diaeresis(a));
    }
    let s = verb_stem(e);
    let out = match vclass_of(&s) {
        VClass::Basic => strong_past(&s).unwrap_or_else(|| format!("{}ne", s)),
        VClass::AStem => {
            // half-strong pasts for formatives are attested individually; default weak
            format!("{}ne", s)
        }
        VClass::UStem => format!("{}ne", s),
    };
    respell(&out)
}

pub fn perfect(e: &Entry) -> String {
    if let Some(a) = e.attested("perfect").filter(|a| is_clean_form(a)) {
        return respell(&strip_diaeresis(a));
    }
    let s = verb_stem(e);
    let class = vclass_of(&s);
    let mut body = match class {
        VClass::Basic => lengthen_base(&s),
        VClass::AStem => {
            let cut = if s.ends_with("ya") { &s[..s.len() - 2] } else { strip_last(&s) };
            lengthen_base(cut)
        }
        VClass::UStem => lengthen_base(strip_last(&s)),
    };
    // (vowel-final stems after cutting): avoid "ii"
    if body.ends_with('i') {
        body.pop();
    }
    let out = match s.chars().next() {
        Some(c) if is_vowel(c) => {
            // reduplicate first two letters (orta- → orortië)
            let two: String = s.chars().take(2).collect();
            format!("{}{}ie", two, body)
        }
        _ => match base_vowel(&s) {
            Some(v) => format!("{}{}ie", v, body),
            None => format!("{}ie", body),
        },
    };
    respell(&out)
}

pub fn gerund(e: &Entry) -> String {
    if let Some(a) = e.attested("gerund").filter(|a| is_clean_form(a)) {
        return respell(&strip_diaeresis(a));
    }
    let s = verb_stem(e);
    let out = match vclass_of(&s) {
        VClass::Basic => format!("{}ie", s),
        VClass::AStem => {
            if s.ends_with("ya") {
                format!("{}ie", &s[..s.len() - 2])
            } else {
                format!("{}ie", strip_last(&s))
            }
        }
        VClass::UStem => format!("{}ye", s),
    };
    respell(&out)
}

pub fn active_participle(e: &Entry) -> String {
    if let Some(a) = e.attested("active-participle").filter(|a| is_clean_form(a)) {
        return respell(&strip_diaeresis(a));
    }
    let s = verb_stem(e);
    respell(&format!("{}ila", s))
}

pub fn passive_participle(e: &Entry) -> String {
    if let Some(a) = e.attested("passive-participle").filter(|a| is_clean_form(a)) {
        return respell(&strip_diaeresis(a));
    }
    let s = verb_stem(e);
    let out = match vclass_of(&s) {
        VClass::Basic => format!("{}ina", s),
        VClass::AStem => {
            if s.ends_with("ya") {
                format!("{}ina", &s[..s.len() - 2])
            } else if e.vclass == vclass::WEAK || e.vclass == vclass::TA_CAUSATIVE {
                format!("{}ina", s)
            } else {
                format!("{}ina", strip_last(&s))
            }
        }
        VClass::UStem => format!("{}na", s),
    };
    respell(&out)
}

pub fn imperative(e: &Entry) -> String {
    format!("á {}", aorist(e))
}

fn tense_form(e: &Entry, t: Tense) -> String {
    match t {
        Tense::Aorist => aorist(e),
        Tense::Present => present(e),
        Tense::Past => past(e),
        Tense::Perfect => perfect(e),
        Tense::Future => future(e),
        Tense::PastImperfect => {
            let p = strip_diaeresis(&present(e));
            // a-stem presents in -ëa lengthen before -në: fárëa → fárëánë
            match p.strip_suffix("ea") {
                Some(b) => format!("{}ëáne", b),
                None => format!("{}ne", p),
            }
        }
        Tense::PastPerfect => format!("{}ne", strip_diaeresis(&perfect(e)).replace("ie", "ié")),
        Tense::FutureImperfect => format!("{}uva", strip_diaeresis(&present(e))),
        Tense::FuturePerfect => format!("{}va", strip_diaeresis(&perfect(e)).replace("ie", "ié")),
    }
}

/// Add a subject suffix to a tensed verb form.
fn add_subject(base: &str, e: &Entry, t: Tense, subj: Person, long: bool) -> String {
    let (short, longf) = subj.subject_suffix();
    let suf = if long { longf } else { short };
    let b = strip_diaeresis(base);
    if t == Tense::Aorist {
        let stem = aorist_suffix_stem(e);
        // attested aorist could have an irregular stem; only use it if it is vowel final
        if !e.attested("aorist").filter(|a| is_clean_form(a)).is_some() || true {
            return format!("{}{}", stem, suf);
        }
    }
    format!("{}{}", b, suf)
}

pub struct VerbSpec {
    pub tense: Tense,
    pub subject: Option<Person>,
    /// Subject is a plural noun (not a pronoun): add -r.
    pub plural_subject: bool,
    pub dual_subject: bool,
    /// Third person object suffix (-s / -t).
    pub object: Option<Person>,
    pub negative: bool,
}

/// Conjugate `e` according to `spec`. Returns one or more words.
pub fn conjugate(e: &Entry, spec: &VerbSpec) -> String {
    let base = tense_form(e, spec.tense);
    if spec.negative {
        // negative verb ui- carries the pronoun; the main verb keeps the tense
        let neg = match spec.subject {
            Some(p) => {
                let (short, _) = p.subject_suffix();
                format!("ui{}", short)
            }
            None if spec.plural_subject => "uir".to_string(),
            None => "ui".to_string(),
        };
        let mut v = base;
        if let Some(o) = spec.object {
            v = with_object(&v, o);
        }
        return format!("{} {}", respell(&neg), v);
    }
    let mut out = match spec.subject {
        Some(p) => {
            let long = spec.object.is_some() || matches!(p, Person::S3 | Person::S3Inan) && spec.object.is_some();
            add_subject(&base, e, spec.tense, p, long)
        }
        None => {
            let b = strip_diaeresis(&base);
            // basic-verb aorist: silë but silir (the -i- stem before any ending)
            let stem = if spec.tense == Tense::Aorist && (spec.plural_subject || spec.dual_subject) { aorist_suffix_stem(e) } else { b };
            if spec.plural_subject {
                format!("{}r", stem)
            } else if spec.dual_subject {
                format!("{}t", stem)
            } else {
                stem
            }
        }
    };
    if let Some(o) = spec.object {
        if spec.subject.is_some() {
            out = format!("{}{}", out, object_suffix(o));
        } else {
            out = with_object(&out, o);
        }
    }
    respell(&out)
}

fn object_suffix(o: Person) -> &'static str {
    match o {
        Person::P3 | Person::P3Inan | Person::D3 => "t",
        _ => "s",
    }
}

fn with_object(v: &str, o: Person) -> String {
    // aorist keeps its final e before an object suffix
    respell(&format!("{}{}", strip_diaeresis(v), object_suffix(o)))
}

/// The copula "to be".
pub fn copula(t: Tense, subj: Option<Person>, plural_subject: bool, negative: bool) -> String {
    let base = if negative {
        match t {
            Tense::Past | Tense::PastImperfect | Tense::PastPerfect => "úne",
            Tense::Perfect => "uie",
            Tense::Future | Tense::FutureImperfect | Tense::FuturePerfect => "úva",
            _ => "ui",
        }
    } else {
        match t {
            Tense::Past | Tense::PastImperfect | Tense::PastPerfect => "náne",
            Tense::Perfect => "anaie",
            Tense::Future | Tense::FutureImperfect | Tense::FuturePerfect => "nauva",
            _ => "ná",
        }
    };
    let out = match subj {
        Some(p) => {
            let (short, long) = p.subject_suffix();
            if base == "ná" {
                match p {
                    Person::S1 => "nányë".to_string(),
                    Person::S2Fam => "natyë".to_string(),
                    Person::S2Pol => "nalyë".to_string(),
                    Person::S3 | Person::S3Inan => "nás".to_string(),
                    Person::P3 | Person::P3Inan => "nar".to_string(),
                    _ => format!("na{}", long),
                }
            } else if base == "ui" {
                match p {
                    Person::P3 | Person::P3Inan => "uir".to_string(),
                    _ => format!("ui{}", short),
                }
            } else {
                match p {
                    Person::P3 | Person::P3Inan => format!("{}r", base),
                    _ => format!("{}{}", base, short),
                }
            }
        }
        None if plural_subject => format!("{}r", base),
        None => base.to_string(),
    };
    respell(&out)
}

/// "there is / exists"
pub fn exist(t: Tense, plural: bool) -> String {
    let base = match t {
        Tense::Past | Tense::PastImperfect | Tense::PastPerfect => "engë",
        Tense::Future | Tense::FutureImperfect | Tense::FuturePerfect => "euva",
        _ => "ëa",
    };
    if plural {
        format!("{}r", base)
    } else {
        base.to_string()
    }
}

// ------------------------------------------------------------------ numerals

pub fn cardinal(n: u64) -> Option<String> {
    let small = |n: u64| -> Option<&'static str> {
        Some(match n {
            0 => "munta",
            1 => "min",
            2 => "atta",
            3 => "neldë",
            4 => "canta",
            5 => "lempë",
            6 => "enquë",
            7 => "otso",
            8 => "tolto",
            9 => "nertë",
            10 => "quëan",
            11 => "minquë",
            12 => "yunquë",
            _ => return None,
        })
    };
    if n <= 12 {
        return small(n).map(|s| s.to_string());
    }
    if n < 100 {
        let tens = match n / 10 {
            1 => "quëan",
            2 => "yúquëan",
            3 => "nelquëan",
            4 => "canquëan",
            5 => "lepenquëan",
            6 => "eneñquëan",
            7 => "otoquëan",
            8 => "tolquëan",
            9 => "nerquëan",
            _ => return None,
        };
        return Some(if n % 10 == 0 { tens.to_string() } else { format!("{} {}", tens, small(n % 10)?) });
    }
    if n < 1000 {
        let h = n / 100;
        let head = if h == 1 { "tuxa".to_string() } else { format!("{} tuxa", small(h)?) };
        return Some(if n % 100 == 0 { head } else { format!("{} {}", head, cardinal(n % 100)?) });
    }
    if n < 1_000_000 {
        let k = n / 1000;
        let head = if k == 1 { "húmë".to_string() } else { format!("{} húmë", cardinal(k)?) };
        return Some(if n % 1000 == 0 { head } else { format!("{} {}", head, cardinal(n % 1000)?) });
    }
    if n < 1_000_000_000 {
        let m = n / 1_000_000;
        let head = if m == 1 { "mindóra".to_string() } else { format!("{} mindóra", cardinal(m)?) };
        return Some(if n % 1_000_000 == 0 { head } else { format!("{} {}", head, cardinal(n % 1_000_000)?) });
    }
    None
}

pub fn ordinal(n: u64) -> Option<&'static str> {
    Some(match n {
        1 => "minya",
        2 => "attëa",
        3 => "nelya",
        4 => "cantëa",
        5 => "lempëa",
        6 => "enquëa",
        7 => "otsëa",
        8 => "toltëa",
        9 => "nertëa",
        10 => "quainëa",
        11 => "minquëa",
        12 => "yunquëa",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(form: &'static str, stem: &'static str, vc: u8) -> Entry {
        Entry { id: 0, form, stem, pos: 0, lang: 0, vclass: vc, mark: 0, refs: 0, gloss: "", infl: vec![], tengwar: "", speech: "" }
    }

    #[test]
    fn nouns() {
        assert_eq!(inflect_noun("elda", "", Number::Pl, Case::Nom, None), "eldar");
        assert_eq!(inflect_noun("lassë", "", Number::Pl, Case::Nom, None), "lassi");
        assert_eq!(inflect_noun("atan", "", Number::Pl, Case::Nom, None), "atani");
        assert_eq!(inflect_noun("oron", "oront-", Number::Pl, Case::Nom, None), "oronti");
        assert_eq!(inflect_noun("cirya", "", Number::Sg, Case::Gen, None), "ciryo");
        assert_eq!(inflect_noun("cirya", "", Number::Pl, Case::Gen, None), "ciryaron");
        assert_eq!(inflect_noun("cirya", "", Number::Sg, Case::Loc, None), "ciryassë");
        assert_eq!(inflect_noun("elen", "", Number::Sg, Case::All, None), "elenna");
        assert_eq!(inflect_noun("atar", "", Number::Sg, Case::Dat, None), "ataren");
        assert_eq!(inflect_noun("cirya", "", Number::Sg, Case::Nom, Some(Person::S1)), "ciryanya");
        assert_eq!(inflect_noun("óma", "", Number::Sg, Case::Gen, Some(Person::S3)), "ómaryo");
        assert_eq!(inflect_noun("atar", "", Number::Sg, Case::Nom, Some(Person::S1)), "atarinya");
        assert_eq!(inflect_noun("ranco", "rancu-", Number::Pl, Case::Nom, None), "ranqui");
        assert_eq!(inflect_noun("alda", "", Number::Du, Case::Nom, None), "aldu");
        assert_eq!(inflect_noun("elda", "", Number::Part, Case::Nom, None), "eldali");
    }

    #[test]
    fn verbs() {
        let mat = entry("mat-", "", vclass::BASIC);
        assert_eq!(aorist(&mat), "matë");
        assert_eq!(present(&mat), "máta");
        assert_eq!(future(&mat), "matuva");
        assert_eq!(past(&mat), "mantë");
        assert_eq!(perfect(&mat), "amátië");
        let fara = entry("fara-", "", vclass::A_VERB);
        assert_eq!(present(&fara), "fárëa");
        assert_eq!(future(&fara), "faruva");
        assert_eq!(past(&fara), "faranë");
        assert_eq!(perfect(&fara), "afárië");
        assert_eq!(gerund(&fara), "farië");
        let orya = entry("orya-", "", vclass::YA_FORMATIVE);
        assert_eq!(perfect(&orya), "orórië");
        let tul = entry("tul-", "", vclass::BASIC);
        assert_eq!(perfect(&tul), "utúlië");
        let spec = VerbSpec { tense: Tense::Aorist, subject: Some(Person::S1), plural_subject: false, dual_subject: false, object: None, negative: false };
        assert_eq!(conjugate(&mat, &spec), "matin");
        let spec = VerbSpec { tense: Tense::Past, subject: None, plural_subject: true, dual_subject: false, object: None, negative: false };
        assert_eq!(conjugate(&tul, &spec), "tuller");
        assert_eq!(copula(Tense::Aorist, Some(Person::S1), false, false), "nányë");
        assert_eq!(adjective_plural("vanya"), "vanyë");
        assert_eq!(adjective_plural("laurëa"), "laurië");
        assert_eq!(intensify("calima", false), "ancalima");
    }
}
