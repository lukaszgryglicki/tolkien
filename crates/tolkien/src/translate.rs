//! Sentence-level translation: POS tagging, shallow parsing and Quenya rendering.
//!
//! Pipeline per line: tokens → items (contractions expanded, multiword
//! expressions merged, unspaced scripts segmented, code/URLs protected) →
//! clauses → Viterbi POS tagging → phrase nodes (NP/VP/PP…) → clause
//! interpretation (subject, tense, objects, negation, questions) → Quenya
//! words, which are written back into the original word slots so that
//! whitespace and punctuation are preserved.

use crate::analyze::{Analysis, Analyzer, Cand, Cat, CATS};
use crate::data::Entry;
use tkdata::fold::fold_key;
use crate::english::{self, Aux, PrepRole, Role};
use crate::quenya::{self, Case, Number, Person, Tense, VerbSpec};
use crate::cjk::{self, PKind};
use crate::tokenize::{self, capitalization, is_hangul, is_kana, is_unspaced_script, latinize_punct, title_case, Cap, Kind};
use std::rc::Rc;
use tkdata::features as f;
use tkdata::{elang, pos, vclass};

pub struct Options {
    pub mark_unknown: bool,
    pub adapt: bool,
    pub numerals: bool,
    pub explain: bool,
}

pub struct Translator<'d> {
    pub an: Analyzer<'d>,
    pub opts: Options,
    in_fence: bool,
    /// The clause being rendered ends in an exclamation mark.
    exclaim: bool,
    pub explain_lines: Vec<String>,
}

// ------------------------------------------------------------------ tags

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tag {
    N = 0,
    V,
    Adj,
    Adv,
    Pron,
    Det,
    Poss,
    Num,
    Prep,
    Conj,
    Intj,
    Aux,
    Cop,
    Neg,
    To,
    Wh,
    Other,
}

const NTAG: usize = 17;
const ALL_TAGS: [Tag; NTAG] = [
    Tag::N, Tag::V, Tag::Adj, Tag::Adv, Tag::Pron, Tag::Det, Tag::Poss, Tag::Num, Tag::Prep, Tag::Conj, Tag::Intj, Tag::Aux, Tag::Cop, Tag::Neg, Tag::To, Tag::Wh, Tag::Other,
];

impl Tag {
    fn i(self) -> usize {
        self as usize
    }
    fn name(self) -> &'static str {
        match self {
            Tag::N => "N",
            Tag::V => "V",
            Tag::Adj => "ADJ",
            Tag::Adv => "ADV",
            Tag::Pron => "PRON",
            Tag::Det => "DET",
            Tag::Poss => "POSS",
            Tag::Num => "NUM",
            Tag::Prep => "PREP",
            Tag::Conj => "CONJ",
            Tag::Intj => "INTJ",
            Tag::Aux => "AUX",
            Tag::Cop => "COP",
            Tag::Neg => "NEG",
            Tag::To => "TO",
            Tag::Wh => "WH",
            Tag::Other => "X",
        }
    }
    fn cat(self) -> Cat {
        match self {
            Tag::N => Cat::N,
            Tag::V | Tag::Aux | Tag::Cop => Cat::V,
            Tag::Adj => Cat::Adj,
            Tag::Adv => Cat::Adv,
            Tag::Pron | Tag::Det | Tag::Poss => Cat::Pron,
            Tag::Num => Cat::Num,
            Tag::Prep | Tag::To => Cat::Prep,
            Tag::Conj => Cat::Conj,
            Tag::Intj => Cat::Intj,
            Tag::Neg | Tag::Wh | Tag::Other => Cat::Other,
        }
    }
}

#[derive(Clone, Debug)]
struct Item {
    text: String,
    kind: Kind,
    cap: Cap,
    /// Synthetic split (contraction piece, script segment): needs a space when emitted next to a word.
    glue: bool,
    /// Pass through untouched (code, URLs, identifiers).
    protected: bool,
    an: Option<Rc<Analysis>>,
    tag: Tag,
    /// Morphological features of the chosen reading.
    tags: u64,
    /// Grammatical role consistent with the chosen tag.
    role: Option<Role>,
    role_key: String,
    out: Option<String>,
    is_name: bool,
    note: String,
    /// Features forced by a separate marker in the source (Japanese/Korean case particles,
    /// Chinese aspect particles): case bits replace the lexical case reading.
    forced: u64,
    /// The word closes its phrase (a case particle followed it).
    phrase_end: bool,
    /// A sentence-final question particle followed this word.
    qmark: bool,
    /// Role decided by a tagging repair ("que" retagged as the relative pronoun), overriding
    /// the emission-based choice.
    pin_role: Option<(Role, String)>,
    /// Position in the line's item vector (set once tokenisation is complete).
    idx: usize,
}

impl Item {
    fn literal(text: &str, kind: Kind) -> Item {
        Item {
            text: text.to_string(),
            kind,
            cap: Cap::Lower,
            glue: false,
            protected: false,
            an: None,
            tag: Tag::Other,
            tags: 0,
            role: None,
            role_key: String::new(),
            out: Some(text.to_string()),
            is_name: false,
            note: String::new(),
            forced: 0,
            phrase_end: false,
            qmark: false,
            pin_role: None,
            idx: 0,
        }
    }
    fn is_word(&self) -> bool {
        matches!(self.kind, Kind::Word | Kind::Number)
    }
    fn role(&self) -> Option<Role> {
        self.role
    }
    fn role_key(&self) -> &str {
        &self.role_key
    }
}

fn up(e: &mut [f32; NTAG], t: Tag, v: f32) {
    if v > e[t.i()] {
        e[t.i()] = v;
    }
}

const INTERJ_KEYS: &[&str] = &[
    "hello", "hi", "hey", "greetings", "goodbye", "bye", "farewell", "thanks", "please", "okay", "ok", "alright", "oh", "ah", "alas", "behold", "lo", "amen", "yes", "yeah", "yep", "indeed", "well",
];
const PRONOUNISH_KEYS: &[&str] = &["nothing", "nobody", "something", "anything", "somebody", "someone", "anyone", "anybody", "everything", "everyone", "everybody"];

fn start_score(t: Tag) -> f32 {
    match t {
        Tag::Det => 6.0,
        Tag::Pron => 6.0,
        Tag::N => 5.0,
        Tag::V => 4.0,
        Tag::Wh => 5.5,
        Tag::Aux => 5.0,
        Tag::Cop => 4.0,
        Tag::Adv => 3.0,
        Tag::Intj => 5.0,
        Tag::Prep => 3.0,
        Tag::Adj => 2.0,
        Tag::Num => 3.0,
        Tag::Conj => 2.5,
        Tag::Poss => 5.0,
        Tag::To => 1.0,
        Tag::Neg => 4.0,
        Tag::Other => 0.0,
    }
}

fn end_score(t: Tag) -> f32 {
    match t {
        Tag::N => 3.0,
        Tag::V => 2.0,
        Tag::Adj => 2.0,
        Tag::Adv => 2.0,
        Tag::Pron => 2.0,
        Tag::Num => 1.0,
        Tag::Intj => 2.0,
        Tag::Det => -8.0,
        Tag::Poss => -8.0,
        Tag::To => -6.0,
        Tag::Prep => -4.0,
        Tag::Conj => -4.0,
        Tag::Aux => -2.0,
        Tag::Neg => -1.0,
        Tag::Wh => -2.0,
        Tag::Cop => 0.0,
        Tag::Other => 0.0,
    }
}

fn trans(a: Tag, b: Tag, foreign: bool) -> f32 {
    use Tag::*;
    if foreign {
        // freer word order: nouns string together, objects follow subject pronouns directly
        match (a, b) {
            (N, N) => return 4.0,
            (Pron, Det) => return 2.0,
            // "dałem mu miecz": an object pronoun directly before the object noun
            (Pron, N) => return 0.0,
            // subject pronoun + object clitic ("ona go", "je le", "yo te")
            (Pron, Pron) => return 3.0,
            _ => {}
        }
    }
    match (a, b) {
        (Det, N) => 10.0,
        (Det, Adj) => 8.0,
        (Det, Num) => 6.0,
        (Det, Det) => -1.0,
        (Det, V) => -15.0,
        (Det, Pron) => -6.0,
        (Det, Prep) => -8.0,
        (Det, Aux) => -10.0,
        (Det, Cop) => -10.0,
        (Det, Neg) => -6.0,
        (Det, Adv) => -2.0,
        (Poss, N) => 10.0,
        (Poss, Adj) => 8.0,
        (Poss, Num) => 5.0,
        (Poss, V) => -15.0,
        (Poss, Det) => -8.0,
        (Adj, N) => 10.0,
        (Adj, Adj) => 2.0,
        (Adj, Conj) => 2.0,
        (Adj, Prep) => 2.0,
        (Adj, V) => -6.0,
        (Adj, Det) => -6.0,
        (Adj, Cop) => 2.0,
        (N, V) => 6.0,
        (N, Prep) => 6.0,
        (N, Conj) => 4.0,
        (N, Cop) => 7.0,
        (N, Aux) => 6.0,
        (N, N) => 1.0,
        (N, Det) => 2.0,
        (N, Adv) => 2.0,
        (N, Adj) => -2.0,
        (N, Pron) => 1.0,
        (N, Wh) => 3.0,
        (N, Num) => 1.0,
        (N, To) => 2.0,
        (N, Neg) => 4.0,
        (Pron, Neg) => 4.0,
        (Adv, Neg) => 2.0,
        (Pron, V) => 10.0,
        (Pron, Aux) => 10.0,
        (Pron, Cop) => 10.0,
        (Pron, N) => -8.0,
        (Pron, Adj) => -4.0,
        (Pron, Prep) => 2.0,
        (Pron, Adv) => 2.0,
        (Pron, Conj) => 2.0,
        (Pron, Det) => -3.0,
        (Aux, V) => 10.0,
        (Aux, Neg) => 8.0,
        (Aux, Adv) => 4.0,
        (Aux, Pron) => 5.0,
        (Aux, Cop) => 8.0,
        (Aux, Aux) => 3.0,
        (Aux, Det) => -6.0,
        (Aux, N) => -6.0,
        (Aux, Adj) => -6.0,
        (Cop, Adj) => 8.0,
        (Cop, Det) => 7.0,
        (Cop, V) => 6.0,
        (Cop, Neg) => 8.0,
        (Cop, Adv) => 5.0,
        (Cop, Prep) => 6.0,
        (Cop, Pron) => 5.0,
        (Cop, N) => 5.0,
        (Cop, Num) => 4.0,
        (Cop, Poss) => 5.0,
        (Cop, Wh) => 3.0,
        (Neg, V) => 10.0,
        (Neg, Adj) => 5.0,
        (Neg, Det) => 4.0,
        (Neg, Adv) => 3.0,
        (Neg, N) => 2.0,
        (Neg, Cop) => 3.0,
        (Neg, Prep) => 2.0,
        (V, Det) => 8.0,
        (V, Pron) => 6.0,
        (V, N) => 6.0,
        (V, Prep) => 6.0,
        (V, Adv) => 6.0,
        (V, To) => 5.0,
        (V, Adj) => 3.0,
        (V, Poss) => 6.0,
        (V, V) => -6.0,
        (V, Conj) => 3.0,
        (V, Neg) => 2.0,
        (V, Num) => 3.0,
        (V, Wh) => 3.0,
        (Prep, Det) => 10.0,
        (Prep, N) => 8.0,
        (Prep, Pron) => 8.0,
        (Prep, Poss) => 8.0,
        (Prep, Adj) => 5.0,
        (Prep, Num) => 5.0,
        (Prep, V) => -10.0,
        (Prep, Wh) => 3.0,
        (Prep, Prep) => -4.0,
        (Prep, Cop) => -8.0,
        (Prep, Aux) => -8.0,
        (To, V) => 12.0,
        (To, Det) => 6.0,
        (To, N) => 4.0,
        (To, Pron) => 5.0,
        (To, Poss) => 5.0,
        (To, Adv) => 1.0,
        (Num, N) => 9.0,
        (Num, Adj) => 4.0,
        (Num, Num) => 1.0,
        (Num, Prep) => 2.0,
        (Conj, V) => 2.0,
        (Conj, Det) => 5.0,
        (Conj, N) => 4.0,
        (Conj, Adj) => 3.0,
        (Conj, Adv) => 2.0,
        (Conj, Pron) => 5.0,
        (Conj, Poss) => 4.0,
        (Wh, Aux) => 8.0,
        (Wh, Cop) => 8.0,
        (Wh, V) => 6.0,
        (Wh, Pron) => 4.0,
        (Wh, N) => 3.0,
        (Wh, Det) => 3.0,
        (Adv, V) => 5.0,
        (Adv, Adj) => 5.0,
        (Adv, Det) => 2.0,
        (Adv, Cop) => 3.0,
        (Adv, Adv) => 2.0,
        (Adv, N) => -2.0,
        (Adv, Pron) => 1.0,
        (Adv, Prep) => 2.0,
        (Intj, _) => 0.5,
        _ => 0.0,
    }
}

fn tense_of(tags: u64) -> Option<Tense> {
    if tags & f::PAST != 0 {
        if tags & f::IMPF != 0 {
            Some(Tense::PastImperfect)
        } else if tags & f::PERF != 0 {
            Some(Tense::PastPerfect)
        } else {
            Some(Tense::Past)
        }
    } else if tags & f::PERF != 0 {
        Some(Tense::Perfect)
    } else if tags & f::FUT != 0 {
        Some(Tense::Future)
    } else if tags & f::IMPF != 0 || (tags & f::PRES != 0 && tags & f::PTCP != 0) {
        Some(Tense::Present)
    } else {
        None
    }
}

fn person_of(tags: u64) -> Option<Person> {
    let pl = tags & f::PL != 0;
    if tags & f::P1 != 0 {
        Some(if pl { Person::P1Ex } else { Person::S1 })
    } else if tags & f::P2 != 0 {
        Some(if pl { Person::P2 } else { Person::S2Pol })
    } else if tags & f::P3 != 0 {
        Some(if pl { Person::P3 } else { Person::S3 })
    } else {
        None
    }
}

pub(crate) fn case_of_tags(tags: u64) -> Option<Case> {
    if tags & f::GEN != 0 {
        Some(Case::Gen)
    } else if tags & f::DAT != 0 {
        Some(Case::Dat)
    } else if tags & f::INS != 0 {
        Some(Case::Ins)
    } else if tags & (f::LOC | f::INE | f::ADE) != 0 {
        Some(Case::Loc)
    } else if tags & (f::ABL | f::ELA) != 0 {
        Some(Case::Abl)
    } else if tags & f::ALL != 0 {
        Some(Case::All)
    } else if tags & f::POSS != 0 {
        Some(Case::Poss)
    } else {
        None
    }
}

/// Rank of a case reading: unmarked readings are preferred (NOM > ACC > DAT > GEN > INS > LOC > ...).
fn case_rank(t: u64) -> u32 {
    if t & f::NOM != 0 {
        0
    } else if t & f::ACC != 0 {
        1
    } else if t & f::VOC != 0 {
        1
    } else if t & f::CASES == 0 {
        2
    } else if t & f::DAT != 0 {
        3
    } else if t & f::GEN != 0 {
        4
    } else if t & f::INS != 0 {
        5
    } else if t & f::ALL != 0 {
        6
    } else if t & f::ABL != 0 {
        7
    } else {
        8
    }
}

/// Choose the tagset of a sense that fits the category, preferring readings
/// compatible with `prefer` (case/number bits of a determiner) and unmarked cases.
fn best_tagset(s: &crate::analyze::Sense, cat: Cat, prefer: u64) -> u64 {
    let verbal = f::TENSES | f::MOODS | f::PERSONS;
    let mut best: Option<(i32, u64)> = None;
    for &t in &s.tagsets {
        let is_verbal = t & verbal != 0;
        let fits = match cat {
            Cat::V => is_verbal || t & (f::NUMBER | f::CASES) == 0,
            Cat::N | Cat::Adj | Cat::Pron | Cat::Num => !is_verbal || t & f::PTCP != 0,
            _ => true,
        };
        let mut score = if fits { 40 } else { 0 } + if t & (f::ALT | f::ARCHAIC) == 0 { 10 } else { 0 };
        if cat == Cat::V && t & f::IMP != 0 && t & f::TENSES == 0 {
            // "ama" (pt/es/it) is both 3sg present and 2sg imperative: running text is mostly indicative
            score -= 1;
        }
        if matches!(cat, Cat::N | Cat::Adj | Cat::Pron) {
            score -= case_rank(t) as i32 * 2;
            // unvocalised Arabic "الملك" is both "the king" and "the kings": singular by default
            if t & f::SG != 0 {
                score += 1;
            }
            if prefer != 0 {
                let pc = prefer & f::CASES;
                let pn = prefer & f::NUMBER;
                let tc = t & f::CASES;
                let tn = t & f::NUMBER;
                if pc != 0 && tc != 0 {
                    score += if pc & tc != 0 { 12 } else { -12 };
                }
                if pn != 0 && tn != 0 {
                    score += if pn & tn != 0 { 6 } else { -6 };
                }
            }
        }
        if best.map(|(bs, _)| score > bs).unwrap_or(true) {
            best = Some((score, t));
        }
    }
    best.map(|(_, t)| t).unwrap_or(0)
}

fn is_third(p: Person) -> bool {
    matches!(p, Person::S3 | Person::S3Inan | Person::P3 | Person::P3Inan)
}

fn pron_case(p: Person, case: Case) -> String {
    match case {
        Case::Nom => p.independent().to_string(),
        Case::Dat | Case::All => p.dative(),
        Case::Gen | Case::Poss => format!("{}o", p.independent()),
        Case::Loc => format!("{}ssë", p.independent()),
        Case::Abl => format!("{}llo", p.independent()),
        Case::Ins => format!("{}nen", p.independent()),
    }
}

const GIVING_VERBS: &[&str] = &["give", "tell", "show", "send", "bring", "offer", "teach", "say", "speak", "write", "sing", "pay", "lend", "owe", "promise", "explain", "hand", "grant", "sell", "read", "present", "deliver"];

// ------------------------------------------------------------------ protection heuristics

pub fn looks_like_code(line: &str) -> bool {
    let t = line.trim_start();
    if t.is_empty() {
        return false;
    }
    let indent = line.len() - t.len();
    let specials = t.chars().filter(|c| matches!(c, '{' | '}' | '(' | ')' | ';' | '=' | '<' | '>' | '[' | ']' | '$' | '|' | '\\')).count();
    let letters = t.chars().filter(|c| c.is_alphabetic()).count().max(1);
    if t.starts_with("#!") || t.starts_with("#include") || t.starts_with("#define") || t.starts_with("import ") || t.starts_with("from ") && t.contains(" import ") {
        return true;
    }
    if t.ends_with(';') || t.ends_with("{") || t.ends_with("});") || t.contains("=>") || t.contains("->") || t.contains("::") || t.contains(");") || t.contains("==") || t.contains("!=") {
        return true;
    }
    if indent >= 4 && specials >= 2 {
        return true;
    }
    if specials * 6 > letters && specials >= 3 {
        return true;
    }
    // e-mail headers, key: value lines with underscores, hex dumps
    if t.chars().filter(|c| c.is_ascii_hexdigit() || c.is_whitespace()).count() == t.chars().count() && t.len() > 20 {
        return true;
    }
    false
}

fn is_fence(line: &str) -> bool {
    let t = line.trim_start();
    (t.starts_with("```") || t.starts_with("~~~")) && line.len() - t.len() <= 3
}

/// A whitespace-delimited chunk that must not be translated.
fn protected_chunk(chunk: &str) -> bool {
    let c = chunk.trim_matches(|c: char| matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\'' | ',' | '.' | ';' | ':' | '!' | '?' | '<' | '>' | '“' | '”' | '‘' | '’' | '«' | '»'));
    if c.is_empty() {
        return false;
    }
    if c.contains("://") || c.starts_with("www.") || c.starts_with("mailto:") {
        return true;
    }
    if c.contains('@') && c.contains('.') {
        return true;
    }
    if c.contains('\\') || c.contains('`') || c.contains('$') || c.contains('%') || c.contains('=') || c.contains('|') || c.contains('~') || c.contains('^') || c.contains('{') || c.contains('}') || c.contains('<') || c.contains('>') || c.contains("::") {
        return true;
    }
    let has_letter = c.chars().any(|ch| ch.is_alphabetic());
    let has_digit = c.chars().any(|ch| ch.is_ascii_digit());
    if c.contains('_') && has_letter {
        return true;
    }
    if c.contains('/') && has_letter && c.len() > 3 && !matches!(c.to_lowercase().as_str(), "and/or" | "or/and" | "he/she" | "she/he" | "s/he" | "his/her" | "her/his" | "yes/no" | "on/off" | "in/out") {
        return true;
    }
    if (c.starts_with('#') || c.starts_with('-') && c.len() > 2 && c.chars().nth(1) == Some('-') || c.starts_with('.') && c.len() > 1 && !c.starts_with("..")) && has_letter && !c.contains(' ') {
        return true;
    }
    if has_letter && has_digit {
        let lower = c.to_lowercase();
        let digits: String = lower.chars().take_while(|ch| ch.is_ascii_digit()).collect();
        let rest = &lower[digits.len()..];
        let ordinal = !digits.is_empty() && matches!(rest, "st" | "nd" | "rd" | "th" | "s");
        let unit = !digits.is_empty() && rest.len() <= 3 && rest.chars().all(|ch| ch.is_ascii_alphabetic());
        if !ordinal && !unit {
            return true;
        }
    }
    // CamelCase identifiers
    let chars: Vec<char> = c.chars().collect();
    if chars.len() > 3 {
        let mut inner_upper = false;
        for w in chars.windows(2) {
            if w[0].is_lowercase() && w[1].is_uppercase() {
                inner_upper = true;
                break;
            }
        }
        if inner_upper && !c.contains('-') {
            return true;
        }
    }
    false
}

/// Byte ranges of the line that must pass through untouched.
fn protected_ranges(line: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    // inline code spans: a run of n backticks is closed by the next run of exactly n
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let mut n = 0;
            while i + n < bytes.len() && bytes[i + n] == b'`' {
                n += 1;
            }
            let mut j = i + n;
            let mut closed = None;
            while j < bytes.len() {
                if bytes[j] == b'`' {
                    let mut m = 0;
                    while j + m < bytes.len() && bytes[j + m] == b'`' {
                        m += 1;
                    }
                    if m == n {
                        closed = Some(j + m);
                        break;
                    }
                    j += m;
                } else {
                    j += 1;
                }
            }
            if let Some(e) = closed {
                out.push((i, e));
                i = e;
            } else {
                i += n;
            }
            continue;
        }
        i += 1;
    }
    // whitespace-delimited chunks
    let mut start: Option<usize> = None;
    for (idx, ch) in line.char_indices().chain(std::iter::once((line.len(), ' '))) {
        if ch.is_whitespace() {
            if let Some(s) = start.take() {
                let chunk = &line[s..idx];
                if protected_chunk(chunk) {
                    // keep surrounding punctuation outside the protected range
                    let lead = chunk.len() - chunk.trim_start_matches(|c: char| matches!(c, '(' | '[' | '{' | '"' | '“' | '«' | '\'' | '‘')).len();
                    let trail = chunk.len() - chunk.trim_end_matches(|c: char| matches!(c, ')' | ']' | '}' | '"' | '”' | '»' | '\'' | '’' | ',' | '.' | ';' | ':' | '!' | '?')).len();
                    if lead + trail < chunk.len() {
                        out.push((s + lead, idx - trail));
                    }
                }
            }
        } else if start.is_none() {
            start = Some(idx);
        }
    }
    out.sort();
    // a code span may contain whitespace-delimited chunks that were also matched: merge
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(out.len());
    for (s, e) in out {
        if let Some(last) = merged.last_mut() {
            if s <= last.1 {
                last.1 = last.1.max(e);
                continue;
            }
        }
        merged.push((s, e));
    }
    merged
}

/// The protected range overlapping the byte span `a..b` (ranges are sorted and disjoint, so
/// only the last range starting before `b` can).
fn overlapping(ranges: &[(usize, usize)], a: usize, b: usize) -> Option<(usize, usize)> {
    let i = ranges.partition_point(|&(s, _)| s < b);
    if i == 0 {
        return None;
    }
    let (s, e) = ranges[i - 1];
    if a < e && b > s { Some((s, e)) } else { None }
}

fn in_ranges(ranges: &[(usize, usize)], a: usize, b: usize) -> Option<(usize, usize)> {
    if a == b {
        return None;
    }
    overlapping(ranges, a, b).filter(|&(s, e)| a >= s && b <= e)
}

fn is_clause_punct(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '.' | '!' | '?' | ';' | ':' | ',' | '…' | '—' | '–' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | '“' | '”' | '«' | '»' | '/' | '|' | '¿' | '¡' | '。' | '、' | '，' | '！' | '？' | '；' | '：' | '「' | '」' | '『' | '』' | '（' | '）' | '【' | '】' | '《' | '》' | '\n'))
}

fn terminal_of(s: &str) -> Option<char> {
    for c in s.chars() {
        match c {
            '?' | '？' | '¿' => return Some('?'),
            '!' | '！' | '¡' => return Some('!'),
            '.' | '。' | '…' => return Some('.'),
            _ => {}
        }
    }
    None
}

/// The first letter has no upper/lower case distinction (CJK, Arabic, Hebrew, Thai...).
fn uncased(s: &str) -> bool {
    s.chars().find(|c| c.is_alphabetic()).map(|c| !c.is_uppercase() && !c.is_lowercase()).unwrap_or(false)
}

fn is_sentence_end(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '.' | '!' | '?' | '…' | '。' | '！' | '？' | ';'))
}

// ------------------------------------------------------------------ translator

impl<'d> Translator<'d> {
    pub fn new(an: Analyzer<'d>, opts: Options) -> Self {
        Translator { an, opts, in_fence: false, exclaim: false, explain_lines: Vec::new() }
    }

    /// Inside a fenced code block (``` … ```): lines pass through untranslated.
    pub fn in_fence(&self) -> bool {
        self.in_fence
    }

    pub fn set_lang(&mut self, lang: &str) {
        self.an.set_lang(lang);
    }

    pub fn lang(&self) -> &str {
        &self.an.lang
    }

    /// Translate one line (without its trailing newline).
    pub fn translate_line(&mut self, line: &str) -> String {
        self.explain_lines.clear();
        if is_fence(line) {
            self.in_fence = !self.in_fence;
            return line.to_string();
        }
        if self.in_fence || line.trim().is_empty() || looks_like_code(line) {
            return line.to_string();
        }
        let mut items = self.build_items(line);
        for (i, it) in items.iter_mut().enumerate() {
            it.idx = i;
        }
        self.process(&mut items);
        let out = assemble(&items, self.lang());
        if self.opts.explain {
            for it in &items {
                if it.is_word() {
                    let key = it.an.as_ref().and_then(|a| a.senses.first().map(|s| s.key.clone())).unwrap_or_default();
                    self.explain_lines.push(format!("{}\t{}\t{}\t{}\t{}", it.text, it.tag.name(), key, it.out.clone().unwrap_or_else(|| "∅".to_string()), it.note));
                }
            }
        }
        out
    }

    fn build_items(&mut self, line: &str) -> Vec<Item> {
        let ranges = protected_ranges(line);
        let toks = tokenize::tokenize(line);
        let base = line.as_ptr() as usize;
        let mut items: Vec<Item> = Vec::with_capacity(toks.len() + 4);
        let mut i = 0;
        while i < toks.len() {
            let t = &toks[i];
            let a = t.text.as_ptr() as usize - base;
            let b = a + t.text.len();
            if let Some((s, e)) = overlapping(&ranges, a, b) {
                // swallow the whole protected range as one item; punctuation glued to its
                // edges ("(`" / "`.") is split off so the code span stays intact
                if a < s {
                    items.push(Item::literal(&line[a..s], Kind::Punct));
                }
                let start = a.max(s);
                let mut j = i;
                let mut end = b.min(e);
                let mut tail = if b > e { Some((e, b, t.kind)) } else { None };
                while tail.is_none() && j + 1 < toks.len() {
                    let n = &toks[j + 1];
                    let na = n.text.as_ptr() as usize - base;
                    let nb = na + n.text.len();
                    if na >= e {
                        break;
                    }
                    j += 1;
                    if nb <= e {
                        end = nb;
                    } else {
                        end = e;
                        tail = Some((e, nb, n.kind));
                    }
                }
                let mut it = Item::literal(&line[start..end], Kind::Word);
                it.protected = true;
                items.push(it);
                if let Some((ts, te, kind)) = tail {
                    items.push(Item::literal(&line[ts..te], kind));
                }
                i = j + 1;
                continue;
            }
            match t.kind {
                Kind::Punct => {
                    let mut it = Item::literal(t.text, t.kind);
                    if let Some(l) = latinize_punct(t.text) {
                        // East Asian punctuation carries no spaces: add one before a following word
                        let next_word = toks.get(i + 1).map(|n| n.kind == Kind::Word).unwrap_or(false);
                        let sentence = l.chars().any(|c| matches!(c, '.' | ',' | '!' | '?' | ':' | ';'));
                        it.out = Some(if next_word && sentence { format!("{} ", l) } else { l });
                    }
                    items.push(it);
                    i += 1;
                }
                Kind::Space => {
                    items.push(Item::literal(t.text, t.kind));
                    i += 1;
                }
                Kind::Number => {
                    let mut it = Item::literal(t.text, Kind::Number);
                    it.out = None;
                    items.push(it);
                    i += 1;
                }
                Kind::Word => {
                    let first = t.text.chars().next().unwrap_or(' ');
                    if is_unspaced_script(first) {
                        self.segment_unspaced(t.text, &mut items);
                        i += 1;
                        continue;
                    }
                    if self.an.lang == "ko" && is_hangul(first) {
                        // 읽지 않는다 / 마시고 싶다 / 놀고 있다: verb + auxiliary in the next word
                        if toks.get(i + 1).map(|n| n.kind == Kind::Space).unwrap_or(false) {
                            if let Some(nx) = toks.get(i + 2) {
                                if nx.kind == Kind::Word && self.korean_aux(t.text, nx.text, &mut items) {
                                    i += 3;
                                    continue;
                                }
                            }
                        }
                        let next_is_word = toks.get(i + 2).map(|n| n.kind == Kind::Word && toks[i + 1].kind == Kind::Space).unwrap_or(false);
                        if self.split_korean(t.text, &mut items, next_is_word) {
                            i += 1;
                            continue;
                        }
                    }
                    // multiword expressions: 3 then 2 words
                    let mut merged = false;
                    for n in [3usize, 2] {
                        let mut idx = i;
                        let mut parts: Vec<&str> = vec![t.text];
                        let mut ok = true;
                        for _ in 1..n {
                            if idx + 2 < toks.len() && toks[idx + 1].kind == Kind::Space && toks[idx + 1].text == " " && toks[idx + 2].kind == Kind::Word {
                                parts.push(toks[idx + 2].text);
                                idx += 2;
                            } else {
                                ok = false;
                                break;
                            }
                        }
                        if !ok {
                            continue;
                        }
                        let last = &toks[idx];
                        let lb = last.text.as_ptr() as usize - base + last.text.len();
                        if in_ranges(&ranges, a, lb).is_some() {
                            continue;
                        }
                        let joined = parts.join(" ");
                        if self.an.known(&joined) {
                            let an = self.an.analyze(&joined);
                            let first_an = self.an.analyze(t.text);
                            let first_is_function = first_an.role.as_ref().map(english::is_function_role).unwrap_or(false);
                            let has_role = matches!(an.role, Some(Role::Prep(_)) | Some(Role::Conj(_)) | Some(Role::Word(_)) | Some(Role::Quant(_)) | Some(Role::Wh(_)) | Some(Role::Pronoun { .. }));
                            let folded = tkdata::fold::fold_en(&joined);
                            let direct = if self.an.is_english() {
                                // the phrase itself ("give up") or an inflected form of it ("looked at")
                                an.senses.iter().any(|s| (s.key == folded && s.weight >= 95) || (s.weight >= 90 && s.key.contains(' ') && s.tagsets.iter().any(|&t| t & (f::PAST | f::PRES | f::P3 | f::PTCP) != 0)))
                                    && an.cands.iter().any(|c| c.base >= 50.0)
                            } else {
                                an.senses.first().map(|s| s.weight >= 85).unwrap_or(false) && an.cands.iter().any(|c| c.base >= 50.0)
                            };
                            // a guessed phrase ("give me" ~ "me") is no expression at all
                            let strong = !an.guessed && (has_role || (direct && !first_is_function));
                            if strong {
                                let text = &line[a..lb];
                                items.push(self.word_item(text, Some(an), false));
                                i = idx + 1;
                                merged = true;
                                break;
                            }
                        }
                    }
                    if merged {
                        continue;
                    }
                    // inflected English phrasal verb whose lemma phrase is the only Quenya route
                    // ("waited for" → "wait for" → hora-, keeping the tense of the inflected word)
                    if self.an.is_english() && i + 2 < toks.len() && toks[i + 1].kind == Kind::Space && toks[i + 1].text == " " && toks[i + 2].kind == Kind::Word {
                        let last = &toks[i + 2];
                        let lb = last.text.as_ptr() as usize - base + last.text.len();
                        if in_ranges(&ranges, a, lb).is_none() {
                            let first_an = self.an.analyze(t.text);
                            // only content words: "can't do" must stay can + not + do
                            let content = first_an.role.is_none() && !t.text.contains(|c| c == '\'' || c == '\u{2019}');
                            let lemma = first_an.senses.first().filter(|s| content && s.weight >= 85 && s.key != tkdata::fold::fold_en(t.text)).map(|s| s.key.clone());
                            if let Some(lemma) = lemma {
                                let joined = format!("{} {}", lemma, tkdata::fold::fold_en(last.text));
                                let no_verb = !first_an.cands.iter().any(|c| c.cat == Cat::V && c.base >= 50.0);
                                if no_verb && self.an.known(&joined) {
                                    let an = self.an.analyze(&joined);
                                    if an.senses.first().map(|s| s.key == joined && s.weight >= 95).unwrap_or(false) && an.cands.iter().any(|c| c.cat == Cat::V && c.base >= 50.0) {
                                        let infl = first_an.senses[0].tagsets.iter().filter(|&&t| t & (f::PAST | f::PRES | f::P3 | f::PTCP | f::PL) != 0).max_by_key(|&&t| {
                                            let rank = if t & f::PAST != 0 && t & f::PTCP == 0 { 3 } else if t & f::PTCP == 0 { 2 } else { 1 };
                                            (rank, -(t.count_ones() as i32))
                                        }).copied().unwrap_or(0);
                                        let text = &line[a..lb];
                                        let mut it = self.word_item(text, Some(an), false);
                                        it.forced |= infl & (f::PAST | f::PRES | f::P3 | f::PL | f::PTCP);
                                        items.push(it);
                                        i += 3;
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                    // contractions (English)
                    if let Some(parts) = self.split_contraction(t.text) {
                        let cap = capitalization(t.text);
                        for (k, p) in parts.iter().enumerate() {
                            let mut it = self.word_item(p, None, k > 0);
                            if k == 0 {
                                it.cap = cap;
                            }
                            items.push(it);
                        }
                        i += 1;
                        continue;
                    }
                    items.push(self.word_item(t.text, None, false));
                    i += 1;
                }
            }
        }
        items
    }

    fn split_contraction(&self, w: &str) -> Option<Vec<String>> {
        if !w.contains('\'') && !w.contains('’') {
            return None;
        }
        if let Some(parts) = english::expand_contraction(w) {
            return Some(parts.iter().map(|s| s.to_string()).collect());
        }
        let norm = w.replace('’', "'");
        if !self.an.is_english() {
            // elision: "l'épée", "dell'anima", "qu'il" — split when the clitic is a known word
            if let Some(k) = norm.find('\'') {
                let (head, rest) = norm.split_at(k + 1);
                if head.len() <= 6 && rest.chars().any(|c| c.is_alphabetic()) && !rest.contains('\'') {
                    let folded = fold_key(&self.an.lang, head);
                    if self.an.data.lex_has(&self.an.lang, &folded) {
                        return Some(vec![head.to_string(), rest.to_string()]);
                    }
                }
            }
            return None;
        }
        for (suf, rep) in [("'ll", "will"), ("'re", "are"), ("'ve", "have"), ("'d", "would"), ("'m", "am"), ("n't", "not")] {
            if let Some(b) = norm.strip_suffix(suf) {
                if b.chars().any(|c| c.is_alphabetic()) {
                    return Some(vec![b.to_string(), rep.to_string()]);
                }
            }
        }
        None
    }

    fn word_item(&mut self, text: &str, an: Option<Rc<Analysis>>, glue: bool) -> Item {
        let an = an.unwrap_or_else(|| self.an.analyze(text));
        Item {
            text: text.to_string(),
            kind: Kind::Word,
            cap: capitalization(text),
            glue,
            protected: false,
            an: Some(an),
            tag: Tag::Other,
            tags: 0,
            role: None,
            role_key: String::new(),
            out: None,
            is_name: false,
            note: String::new(),
            forced: 0,
            phrase_end: false,
            qmark: false,
            pin_role: None,
            idx: 0,
        }
    }

    /// An item standing for an English function word ("also", "and") that a particle expressed.
    fn function_item(&mut self, text: &str, key: &str) -> Item {
        let an = self.an.analyze_en(key);
        let mut it = self.word_item(text, Some(an), true);
        it.role_key = key.to_string();
        it
    }

    /// Index of the first item of the phrase headed by `head` (adjectives, genitives...).
    fn phrase_start(items: &[Item], head: usize, floor: usize) -> usize {
        let mut j = head;
        while j > floor && items[j - 1].kind == Kind::Word && !items[j - 1].phrase_end && !items[j - 1].protected {
            let prev = &items[j - 1];
            // a function word we inserted ("with", "and") or a verb ends the phrase
            let functional = prev.an.as_ref().map(|a| a.roles.iter().any(|(r, _, rel)| *rel >= 0.9 && matches!(r, Role::Conj(_) | Role::Word(_) | Role::Aux(_) | Role::Copula | Role::Neg | Role::Pronoun { .. }))).unwrap_or(false);
            let verbal = prev.an.as_ref().map(|a| a.cands.first().map(|c| matches!(c.cat, Cat::V | Cat::Prep | Cat::Conj)).unwrap_or(false) && a.cands.iter().all(|c| !matches!(c.cat, Cat::N | Cat::Adj | Cat::Pron | Cat::Num))).unwrap_or(false);
            if functional || verbal || prev.forced & f::CASES != 0 && prev.forced & f::GEN == 0 {
                break;
            }
            j -= 1;
        }
        j
    }

    /// Can this piece of an unspaced run be analysed (lexicon or verified morphology)?
    fn piece_quality(&mut self, piece: &str) -> Option<bool> {
        if self.an.known(piece) {
            let an = self.an.analyze(piece);
            if an.guessed && piece.chars().count() >= 2 && piece.chars().all(tokenize::is_han) {
                return None;
            }
            // 誰が: a guessed or phrasal entry that is really word + particle
            if self.an.lang == "ja" && piece.chars().count() >= 2 {
                let last = piece.chars().last().unwrap_or(' ');
                if "がをにはのとでへもか".contains(last) {
                    let prefix = &piece[..piece.len() - last.len_utf8()];
                    if self.an.known(prefix) && (an.guessed || an.senses.iter().all(|s| s.pos == pos::PHRASE)) {
                        return None;
                    }
                }
            }
            return Some(!an.is_empty());
        }
        let last = piece.chars().last().unwrap_or(' ');
        if self.an.lang == "ja" && (is_kana(last) || matches!(last, '々' | '達')) && piece.chars().count() >= 2 {
            let an = self.an.analyze(piece);
            if !an.is_empty() {
                return Some(true);
            }
        }
        None
    }

    /// Apply a particle to the phrase it closes.
    fn apply_particle(&mut self, items: &mut Vec<Item>, run_items_start: usize, text: &str, part: &cjk::Particle, next_is_word: bool, pending_obj: &mut bool) {
        let last_word = items.iter().rposition(|it| it.is_word() && it.kind == Kind::Word && !it.protected).filter(|&k| k >= run_items_start);
        match part.kind {
            PKind::Case => {
                if let Some(k) = last_word {
                    items[k].forced = (items[k].forced & !f::CASES) | part.case;
                    items[k].phrase_end = true;
                }
            }
            PKind::Gen => {
                if let Some(k) = last_word {
                    // 起こったの: の after a finite verb nominalises the clause, it is no genitive
                    let verbal_top = items[k]
                        .an
                        .as_ref()
                        .and_then(|a| a.senses.first())
                        .map(|s| s.pos == pos::VERB || s.tagsets.iter().any(|&t| t & (f::PAST | f::PRES | f::FUT) != 0 && t & f::PTCP == 0))
                        .unwrap_or(false);
                    let nominal = !verbal_top && items[k].an.as_ref().map(|a| a.cands.iter().any(|c| matches!(c.cat, Cat::N | Cat::Pron)) || a.cands.is_empty()).unwrap_or(true);
                    if nominal {
                        items[k].forced = (items[k].forced & !f::CASES) | f::GEN;
                    }
                }
            }
            PKind::Question => {
                if text.starts_with("です") || text.starts_with("でした") {
                    let mut cop = self.function_item(text, "be");
                    cop.forced |= f::FORMAL | if text.starts_with("でした") { f::PAST } else { f::PRES };
                    cop.qmark = true;
                    if let Some(k) = last_word {
                        items[k].phrase_end = true;
                    }
                    items.push(cop);
                } else if let Some(k) = last_word {
                    items[k].qmark = true;
                }
            }
            PKind::Drop | PKind::AdvMarker => {}
            PKind::Word(w) => {
                if let Some(k) = last_word {
                    if part.case != 0 {
                        items[k].forced = (items[k].forced & !f::CASES) | part.case;
                    }
                    items[k].phrase_end = true;
                }
                let it = self.function_item(text, w);
                if matches!(w, "than" | "like" | "with" | "from" | "until" | "to") {
                    let at = last_word.map(|k| Self::phrase_start(items, k, run_items_start)).unwrap_or(items.len());
                    items.insert(at, it);
                } else {
                    items.push(it);
                }
            }
            PKind::AndWith => {
                if let Some(k) = last_word {
                    items[k].phrase_end = true;
                }
                if next_is_word {
                    let it = self.function_item(text, "and");
                    items.push(it);
                } else {
                    // comitative: the postposition becomes a preposition before its phrase
                    let it = self.function_item(text, "with");
                    let at = last_word.map(|k| Self::phrase_start(items, k, run_items_start)).unwrap_or(items.len());
                    items.insert(at, it);
                }
            }
            PKind::ObjNext => {
                // after a numeral or demonstrative 把 is a measure word, not the object marker
                let classifier = last_word.map(|k| items[k].text.chars().all(|c| "一二三四五六七八九十百千兩两几幾這这那每半".contains(c))).unwrap_or(false);
                if !classifier {
                    if let Some(k) = last_word {
                        items[k].phrase_end = true;
                    }
                    *pending_obj = true;
                }
            }
            PKind::Copula(tags) => {
                if let Some(k) = last_word {
                    items[k].phrase_end = true;
                }
                let mut cop = self.function_item(text, "be");
                cop.forced |= tags;
                if tags & f::FORMAL != 0 && text.ends_with('か') {
                    cop.qmark = true;
                }
                items.push(cop);
            }
            PKind::Past => {
                if let Some(k) = last_word {
                    let verbal = items[k].an.as_ref().map(|a| a.cands.iter().any(|c| c.cat == Cat::V)).unwrap_or(false);
                    if verbal {
                        items[k].forced |= f::PAST;
                    }
                }
            }
            PKind::Plural => {
                if let Some(k) = last_word {
                    items[k].forced |= f::PL;
                }
            }
        }
    }

    /// Least-cost segmentation of a run written without word spaces (Chinese, Japanese,
    /// Thai...): dictionary words, verified verb conjugations and grammatical particles.
    fn piece_is_phrase(&mut self, piece: &str) -> bool {
        let an = self.an.analyze(piece);
        an.senses.first().map(|s| s.key.contains(' ')).unwrap_or(false)
    }

    fn segment_unspaced(&mut self, run: &str, items: &mut Vec<Item>) {
        // a run right after a closing quote may start with a particle (」と言った)
        let after_quote = items.last().map(|it| it.kind == Kind::Punct && it.text.chars().all(|c| matches!(c, '」' | '』' | '"' | '”' | '）' | ')' | '»'))).unwrap_or(false);
        #[derive(Clone, Copy)]
        enum PieceKind {
            Lex,
            Particle(usize),
            Unknown,
        }
        let lang = self.an.lang.clone();
        let particles = cjk::particles(&lang);
        let chars: Vec<(usize, char)> = run.char_indices().collect();
        let n = chars.len();
        let byte_at = |i: usize| if i < n { chars[i].0 } else { run.len() };
        let mut best: Vec<(f32, usize, PieceKind)> = vec![(f32::MAX, 0, PieceKind::Unknown); n + 1];
        best[0] = (0.0, 0, PieceKind::Unknown);
        for i in 0..n {
            let base = best[i].0;
            if base == f32::MAX {
                continue;
            }
            if i > 0 || after_quote {
                for (pi, part) in particles.iter().enumerate() {
                    let plen = part.text.chars().count();
                    if i + plen > n || &run[byte_at(i)..byte_at(i + plen)] != part.text {
                        continue;
                    }
                    if part.final_only && i + plen != n {
                        continue;
                    }
                    let c = base + part.cost;
                    if c < best[i + plen].0 {
                        best[i + plen] = (c, i, PieceKind::Particle(pi));
                    }
                }
            }
            let max = (n - i).min(12);
            for len in 1..=max {
                let piece = &run[byte_at(i)..byte_at(i + len)];
                let Some(content) = self.piece_quality(piece) else { continue };
                let mut cost = 1.0;
                if len == 1 {
                    cost += if is_kana(chars[i].1) { 0.9 } else { 0.25 };
                }
                if !content {
                    cost += 0.6;
                }
                if len >= 2 && self.piece_is_phrase(piece) {
                    // multi-word glosses ("such as" for と言った) are weaker than particle + word
                    cost += 0.8;
                }
                if len >= 2 && cjk::zh_no_compound(piece) {
                    cost += 5.0;
                }
                if len == 2 && cjk::localizer_prep(&piece[..chars[i].1.len_utf8()]).is_some() && "にではへをがのも".contains(chars[i + 1].1) {
                    // 上に = localizer + particle, not one word
                    cost += 1.5;
                }
                // ties: prefer longer words early (forward maximum matching)
                cost -= 0.001 * (len * len) as f32 * (n - i) as f32;
                let c = base + cost;
                if c < best[i + len].0 {
                    best[i + len] = (c, i, PieceKind::Lex);
                }
            }
            let c = base + 3.0;
            if c < best[i + 1].0 {
                best[i + 1] = (c, i, PieceKind::Unknown);
            }
        }
        let mut pieces: Vec<(usize, usize, PieceKind)> = Vec::new();
        let mut pos = n;
        while pos > 0 {
            let (_, prev, kind) = best[pos];
            pieces.push((prev, pos, kind));
            pos = prev;
        }
        pieces.reverse();
        let start_len = items.len();
        let mut pending_obj = false;
        for (pi, &(a, b, kind)) in pieces.iter().enumerate() {
            let piece = &run[byte_at(a)..byte_at(b)];
            match kind {
                PieceKind::Lex | PieceKind::Unknown => {
                    // 木の下 / 桌子上: a localizer after a noun is a preposition before it
                    if let Some(prep) = cjk::localizer_prep(piece).filter(|_| pi > 0 && !matches!(lang.as_str(), "ko")) {
                        let prev_ok = matches!(pieces[pi - 1].2, PieceKind::Lex) || (lang == "ja" && matches!(pieces[pi - 1].2, PieceKind::Particle(q) if particles[q].kind == PKind::Gen));
                        let last_word = items.iter().rposition(|it| it.kind == Kind::Word).filter(|&k| k >= start_len);
                        let prev_nominal = last_word.map(|k| items[k].an.as_ref().map(|an| an.cands.iter().any(|c| matches!(c.cat, Cat::N | Cat::Pron)) || an.name).unwrap_or(false)).unwrap_or(false);
                        if prev_ok && prev_nominal {
                            let k = last_word.unwrap();
                            items[k].forced &= !f::GEN;
                            items[k].phrase_end = true;
                            let at = Self::phrase_start(items, k, start_len);
                            let it = self.function_item(piece, prep);
                            // 在森林里: the localizer refines the preposition already there
                            let prev_prep = at > start_len && items[at - 1].kind == Kind::Word && items[at - 1].an.as_ref().map(|a| a.cands.first().map(|c| c.cat == Cat::Prep).unwrap_or(false) || a.roles.iter().any(|(_, key, rel)| *rel >= 0.9 && matches!(key.as_str(), "in" | "at" | "on"))).unwrap_or(false);
                            if prev_prep {
                                if prep != "in" {
                                    let text = items[at - 1].text.clone();
                                    items[at - 1] = self.function_item(&text, prep);
                                }
                            } else {
                                items.insert(at, it);
                            }
                            continue;
                        }
                    }
                    // Chinese measure word between a numeral and its noun: 一本書
                    if b - a == 1 && cjk::is_zh_classifier(chars[a].1) && pi + 1 < pieces.len() {
                        let prev_num = items.last().map(|it| it.kind == Kind::Number || it.text.chars().all(|c| "一二三四五六七八九十百千萬万兩两几幾這这那每半".contains(c))).unwrap_or(false);
                        if prev_num && matches!(pieces[pi + 1].2, PieceKind::Lex) {
                            if items.last().map(|it| it.text == "一").unwrap_or(false) {
                                // 一本書 "a book": the numeral is an article
                                items.pop();
                                let art = self.function_item("一", "a");
                                items.push(art);
                            }
                            continue;
                        }
                    }
                    let mut it = self.word_item(piece, None, true);
                    if pending_obj {
                        it.forced = (it.forced & !f::CASES) | f::ACC;
                        it.phrase_end = true;
                        pending_obj = false;
                    }
                    // desiderative 飲みたい: "want to drink"
                    let desire = it.an.as_ref().and_then(|a| a.senses.first()).map(|s| s.tagsets.iter().any(|&t| t & f::OPT != 0)).unwrap_or(false);
                    if desire {
                        let tags = it.an.as_ref().and_then(|a| a.senses.first()).and_then(|s| s.tagsets.iter().find(|&&t| t & f::OPT != 0).copied()).unwrap_or(0);
                        let mut want = self.function_item(piece, "want");
                        want.forced |= tags & (f::PAST | f::NEG | f::FORMAL);
                        items.push(want);
                        let to = self.function_item(piece, "to");
                        items.push(to);
                        it.forced |= f::INF;
                    }
                    items.push(it);
                }
                PieceKind::Particle(k) => {
                    let part = particles[k];
                    if pi == 0 && after_quote && matches!(part.kind, PKind::AndWith) {
                        continue; // quotative と: "…" he said
                    }
                    let mut next_is_word = pieces.get(pi + 1).map(|&(_, _, k)| matches!(k, PieceKind::Lex | PieceKind::Unknown)).unwrap_or(false);
                    if next_is_word && matches!(part.kind, PKind::AndWith) {
                        // 友達と東京に行く: "with a friend to Tokyo" — coordination only when the
                        // conjoined phrase is itself a subject/object (王と女王が)
                        let after_next = pieces.get(pi + 2).map(|&(_, _, k)| k);
                        next_is_word = match after_next {
                            Some(PieceKind::Particle(q)) => particles[q].case & (f::NOM | f::ACC | f::GEN) != 0 || matches!(particles[q].kind, PKind::Gen | PKind::Plural),
                            Some(PieceKind::Lex) | Some(PieceKind::Unknown) | None => true,
                        };
                    }
                    self.apply_particle(items, start_len, piece, &part, next_is_word, &mut pending_obj);
                }
            }
        }
    }

    /// Korean periphrastic verb forms spread over two words: V-지 않다 (negation),
    /// V-고 싶다 (desiderative), V-고 있다 (progressive).
    fn korean_aux(&mut self, word: &str, next: &str, items: &mut Vec<Item>) -> bool {
        if !word.chars().all(is_hangul) || !next.chars().all(is_hangul) {
            return false;
        }
        let (kind, stem) = if let Some(st) = word.strip_suffix('지') {
            if !next.starts_with('않') {
                return false;
            }
            ("neg", st)
        } else if let Some(st) = word.strip_suffix('고') {
            if next.starts_with('싶') {
                ("want", st)
            } else if next.starts_with('있') {
                ("prog", st)
            } else {
                return false;
            }
        } else {
            return false;
        };
        if stem.is_empty() {
            return false;
        }
        let lemma = format!("{}다", stem);
        if !self.an.known(&lemma) {
            return false;
        }
        let an = self.an.analyze(&lemma);
        if !an.cands.iter().any(|c| c.cat == Cat::V) {
            return false;
        }
        let aux_tail = &next[next.char_indices().nth(1).map(|(i, _)| i).unwrap_or(next.len())..];
        let mut tags = 0u64;
        if aux_tail.contains('었') || aux_tail.contains('았') {
            tags |= f::PAST;
        } else {
            tags |= f::PRES;
        }
        if aux_tail.contains("습니") || aux_tail.contains("어요") || aux_tail.contains("아요") {
            tags |= f::FORMAL;
        }
        let mut it = self.word_item(&lemma, Some(an), true);
        it.text = format!("{} {}", word, next);
        match kind {
            "neg" => it.forced |= tags | f::NEG,
            "prog" => it.forced |= tags | f::IMPF,
            _ => {
                let mut want = self.function_item(next, "want");
                want.forced |= tags;
                items.push(want);
                let to = self.function_item(next, "to");
                items.push(to);
                it.forced |= f::INF;
            }
        }
        items.push(it);
        true
    }

    /// Korean: strip postpositions and attached copulas from a space-delimited word.
    fn split_korean(&mut self, word: &str, items: &mut Vec<Item>, next_is_word: bool) -> bool {
        if !word.chars().all(is_hangul) || word.chars().count() < 2 {
            return false;
        }
        let whole_known = self.an.known(word);
        let whole_an = if whole_known { Some(self.an.analyze(word)) } else { None };
        let whole_verbal = whole_an.as_ref().map(|a| a.cands.iter().any(|c| c.cat == Cat::V)).unwrap_or(false);
        let whole_w = whole_an.as_ref().and_then(|a| a.senses.iter().map(|s| s.weight).max()).unwrap_or(0);
        // attached copula: 왕이다, 왕입니다
        for (suf, tags) in cjk::KO_COPULA {
            if let Some(stem) = word.strip_suffix(suf) {
                if !stem.is_empty() && self.an.known(stem) && !whole_verbal {
                    let mut it = self.word_item(stem, None, true);
                    it.phrase_end = true;
                    items.push(it);
                    let mut cop = self.function_item(suf, "be");
                    cop.forced |= tags;
                    if *suf == "입니까" {
                        cop.qmark = true;
                    }
                    items.push(cop);
                    return true;
                }
            }
        }
        let mut cands: Vec<&cjk::Particle> = cjk::KO.iter().collect();
        cands.sort_by(|a, b| b.text.len().cmp(&a.text.len()));
        for part in cands {
            let Some(stem) = word.strip_suffix(part.text) else { continue };
            if stem.is_empty() {
                continue;
            }
            let stem_known = self.an.known(stem);
            let stem_an = if stem_known { Some(self.an.analyze(stem)) } else { None };
            let stem_nominal = stem_an.as_ref().map(|a| a.cands.iter().any(|c| matches!(c.cat, Cat::N | Cat::Pron)) || a.cands.is_empty() || a.name).unwrap_or(false);
            let stem_pron = stem_an.as_ref().map(|a| a.cands.iter().any(|c| c.cat == Cat::Pron) || matches!(a.role, Some(Role::Pronoun { .. }))).unwrap_or(false);
            let stem_w = stem_an.as_ref().and_then(|a| a.senses.iter().map(|s| s.weight).max()).unwrap_or(0);
            let ok = if whole_known {
                // "검을": sword + object marker rather than a rare adnominal of "black"
                stem_pron || (stem_nominal && (!whole_verbal || stem_w > whole_w) && (stem.chars().count() >= 2 || whole_verbal || stem_w > whole_w))
            } else {
                stem_known && stem_nominal
            };
            if !ok {
                continue;
            }
            let it = self.word_item(stem, None, true);
            items.push(it);
            let start = items.len() - 1;
            let mut pending = false;
            self.apply_particle(items, start, part.text, part, next_is_word, &mut pending);
            return true;
        }
        false
    }

    // -------------------------------------------------------------- clause processing

    fn process(&mut self, items: &mut Vec<Item>) {
        // segments delimited by clause punctuation
        let n = items.len();
        // q_ahead[i]: a question mark follows position i before the sentence ends
        let mut q_ahead = vec![false; n + 1];
        let mut cur = false;
        for k in (0..n).rev() {
            if items[k].kind == Kind::Punct {
                if items[k].text.contains('?') || items[k].text.contains('？') {
                    cur = true;
                } else if is_sentence_end(&items[k].text) {
                    cur = false;
                }
            }
            q_ahead[k] = cur;
        }
        let mut seg: Vec<usize> = Vec::new();
        let mut i = 0;
        let mut sentence_first = true;
        while i <= n {
            let boundary = i == n || (items[i].kind == Kind::Punct && is_clause_punct(&items[i].text));
            if boundary {
                if !seg.is_empty() {
                    // sentence terminal: first terminal punctuation after this segment before the next word
                    let mut terminal = None;
                    let mut j = i;
                    while j < n && !items[j].is_word() {
                        if items[j].kind == Kind::Punct {
                            if let Some(t) = terminal_of(&items[j].text) {
                                terminal = Some(t);
                                break;
                            }
                        }
                        j += 1;
                    }
                    // question mark anywhere later in the sentence
                    let mut question = terminal == Some('?') || q_ahead[i];
                    let seg_copy = std::mem::take(&mut seg);
                    if seg_copy.iter().any(|&k| items[k].qmark) {
                        question = true;
                        // 誰ですか。 → the full stop becomes a question mark
                        let mut j = i;
                        while j < n && !items[j].is_word() {
                            if items[j].kind == Kind::Punct && matches!(items[j].out.as_deref().unwrap_or(items[j].text.as_str()), "." | "。") {
                                items[j].out = Some("?".to_string());
                                break;
                            }
                            j += 1;
                        }
                    }
                    self.process_segment(items, &seg_copy, terminal, question, sentence_first);
                    sentence_first = false;
                }
                if i < n && is_sentence_end(&items[i].text) {
                    sentence_first = true;
                }
                i += 1;
                continue;
            }
            if items[i].is_word() {
                seg.push(i);
            }
            i += 1;
        }
    }

    fn process_segment(&mut self, items: &mut Vec<Item>, idxs: &[usize], terminal: Option<char>, question: bool, sentence_first: bool) {
        self.tag_segment(items, idxs);
        // split into clauses at coordinating conjunctions joining two verb groups and at relative words
        let has_verb = |items: &Vec<Item>, r: &[usize]| r.iter().any(|&k| matches!(items[k].tag, Tag::V | Tag::Cop | Tag::Aux));
        let mut clauses: Vec<Vec<usize>> = Vec::new();
        let mut cur: Vec<usize> = Vec::new();
        // 你为什么哭 / 당신은 누구입니까: the question word stays in place in the source
        let wh_in_situ = matches!(self.lang(), "zh" | "yue" | "lzh" | "nan" | "hak" | "ja" | "ko");
        for (p, &k) in idxs.iter().enumerate() {
            let t = items[k].tag;
            // "that" tagged Conj is the complementizer ("I know that he will come")
            let conj_like = matches!(items[k].role(), Some(Role::Conj(_)) | Some(Role::Demonstrative { .. }));
            if p > 0 && t == Tag::Conj && conj_like && has_verb(items, &cur) && has_verb(items, &idxs[p + 1..]) {
                clauses.push(std::mem::take(&mut cur));
            } else if p > 0 && t == Tag::Wh && !cur.is_empty() && !wh_in_situ {
                clauses.push(std::mem::take(&mut cur));
            }
            cur.push(k);
        }
        if !cur.is_empty() {
            clauses.push(cur);
        }
        let nclauses = clauses.len();
        for (ci, cl) in clauses.into_iter().enumerate() {
            let last = ci + 1 == nclauses;
            self.render_clause(items, &cl, if last { terminal } else { None }, question, sentence_first && ci == 0, ci > 0);
        }
    }

    fn emissions(&self, it: &Item) -> [f32; NTAG] {
        use Tag::*;
        let mut e = [-6.0f32; NTAG];
        if it.kind == Kind::Number {
            up(&mut e, Num, 10.0);
            return e;
        }
        if it.protected {
            up(&mut e, Other, 12.0);
            return e;
        }
        let Some(a) = it.an.as_ref() else {
            up(&mut e, Other, 5.0);
            return e;
        };
        if a.name && a.is_empty() {
            up(&mut e, N, 16.0);
            return e;
        }
        let english = self.an.is_english();
        for (r, key, rel) in &a.roles {
            let mut re = [-6.0f32; NTAG];
            role_emissions(&mut re, r, key, english);
            for t in 0..NTAG {
                if re[t] > -6.0 {
                    let v = if *rel >= 0.999 { re[t] } else { re[t] * rel.max(0.4) };
                    if v > e[t] {
                        e[t] = v;
                    }
                }
            }
            let _ = key;
        }
        // only a well-supported function reading caps the content readings
        let has_role = a.roles.iter().any(|(_, _, rel)| *rel >= 0.75);
        let numeric = a.roles.iter().any(|(r, _, _)| matches!(r, Role::Number(_)));
        let max = a.cat_scores.iter().cloned().fold(0.0f32, f32::max);
        // an article is never a content word ("the" → ina "(stronger) the" is not an adjective)
        let article = a.roles.first().map(|(r, _, rel)| matches!(r, Role::DefArt | Role::IndefArt) && *rel >= 0.999).unwrap_or(false);
        if max > 0.0 && !numeric && !article {
            for (i, &s) in a.cat_scores.iter().enumerate() {
                if s <= 0.0 {
                    continue;
                }
                let t = match CATS[i] {
                    Cat::N => N,
                    Cat::V => V,
                    Cat::Adj => Adj,
                    Cat::Adv => Adv,
                    Cat::Pron => Pron,
                    Cat::Num => Num,
                    Cat::Prep => Prep,
                    Cat::Conj => Conj,
                    Cat::Intj => Intj,
                    Cat::Other => Other,
                };
                let rel = (s / max) * (s / max);
                let v = if CATS[i] == Cat::Other {
                    2.0 + 3.0 * rel
                } else if has_role {
                    2.0 + 4.0 * rel
                } else {
                    3.0 + 7.0 * rel
                };
                up(&mut e, t, v);
            }
        }
        if let Some(s) = a.senses.first() {
            let t = s.tags_any();
            if t & f::PL != 0 && e[N.i()] > -6.0 {
                e[N.i()] += 2.0;
            }
            if t & (f::PAST | f::PTCP | f::INF | f::IMP) != 0 && e[V.i()] > -6.0 {
                e[V.i()] += 3.0;
            }
            if t & f::P3 != 0 && t & f::PRES != 0 && e[V.i()] > -6.0 {
                e[V.i()] += 1.5;
            }
            // "the given key": a past participle is also an adjective
            if t & f::PTCP != 0 && t & f::PAST != 0 && e[V.i()] > -6.0 && a.role.is_none() {
                let v = e[V.i()] - 2.5;
                if v > e[Adj.i()] {
                    e[Adj.i()] = v;
                }
            }
            if t & (f::CMP | f::SUP) != 0 && e[Adj.i()] > -6.0 {
                e[Adj.i()] += 2.0;
            }
            let cap = if has_role { 6.5 } else { 8.0 };
            // the section a function word was listed under says nothing about its use
            let functional = english::role(&s.key).map(|r| english::is_function_role(&r)).unwrap_or(false);
            match s.pos {
                _ if functional => {}
                pos::NOUN => up(&mut e, N, cap),
                pos::NAME => up(&mut e, N, cap + 0.5),
                pos::VERB => up(&mut e, V, cap),
                pos::ADJ => up(&mut e, Adj, cap),
                pos::ADV => up(&mut e, Adv, cap + 0.5),
                _ => {}
            }
        }
        if a.cands.is_empty() && a.role.is_none() {
            up(&mut e, N, if it.cap != Cap::Lower { 8.0 } else { 6.0 });
            up(&mut e, Adj, 2.0);
            up(&mut e, V, 2.0);
        }
        e
    }

    fn tag_segment(&self, items: &mut [Item], idxs: &[usize]) {
        let n = idxs.len();
        if n == 0 {
            return;
        }
        let foreign = !self.an.is_english();
        let mut em: Vec<[f32; NTAG]> = idxs.iter().map(|&i| self.emissions(&items[i])).collect();
        for (k, &i) in idxs.iter().enumerate() {
            let it = &items[i];
            if it.forced & f::CASES != 0 || it.forced & f::PL != 0 {
                // a case particle followed: nominal reading
                for t in [Tag::N, Tag::Pron] {
                    if em[k][t.i()] > -6.0 {
                        em[k][t.i()] += 5.0;
                    }
                }
                let pronominal = it.an.as_ref().map(|a| a.roles.iter().any(|(r, _, rel)| matches!(r, Role::Pronoun { .. }) && *rel >= 0.9)).unwrap_or(false);
                if pronominal && em[k][Tag::Pron.i()] > -6.0 {
                    em[k][Tag::Pron.i()] += 3.0;
                }
                for t in [Tag::V, Tag::Aux, Tag::Cop, Tag::Adv, Tag::Prep, Tag::Conj, Tag::Intj] {
                    em[k][t.i()] -= 5.0;
                }
            }
            if it.forced & f::PAST != 0 && em[k][Tag::V.i()] > -6.0 {
                em[k][Tag::V.i()] += 5.0;
            }
        }
        if foreign {
            // freer word order: trust the lexicon more than the (English-shaped) transitions
            for e in em.iter_mut() {
                for v in e.iter_mut() {
                    if *v > -6.0 {
                        *v *= 1.6;
                    }
                }
            }
        }
        // a confidently known word does not drift into an unevidenced tag just because the
        // transitions like it there ("me and smiled": "and" is never an auxiliary)
        for e in em.iter_mut() {
            let max = e.iter().cloned().fold(f32::MIN, f32::max);
            if max >= 8.0 {
                for v in e.iter_mut() {
                    if *v <= -6.0 {
                        *v = -14.0;
                    }
                }
            }
        }
        if std::env::var_os("TOLKIEN_DEBUG").is_some() {
            for (k, &i) in idxs.iter().enumerate() {
                let mut parts: Vec<String> = Vec::new();
                for t in 0..NTAG {
                    if em[k][t] > -6.0 {
                        parts.push(format!("{:?}={:.1}", ALL_TAGS[t], em[k][t]));
                    }
                }
                eprintln!("emit {:<16} {}", items[i].text, parts.join(" "));
            }
        }
        // a particle closes its phrase: what follows starts a new one (剣を|与えた, 딸은|아름답다)
        let closed: Vec<bool> = idxs.iter().map(|&i| items[i].phrase_end).collect();
        let mut path = viterbi(&em, foreign, &closed);
        // an isolating language (Chinese 給 "give"/"to") needs a verb somewhere: when the
        // chosen path has none, promote the word whose verbal reading loses least
        if foreign && n >= 3 && !path.iter().any(|t| matches!(t, Tag::V | Tag::Cop | Tag::Aux)) {
            let mut cand: Option<(usize, f32)> = None;
            for k in 0..n {
                let gain = em[k][Tag::V.i()] - em[k][path[k].i()];
                if em[k][Tag::V.i()] > 8.0 && gain > -1.5 && cand.map(|(_, g)| gain > g).unwrap_or(true) {
                    cand = Some((k, gain));
                }
            }
            if let Some((k, _)) = cand {
                em[k][Tag::V.i()] += 8.0;
                path = viterbi(&em, foreign, &closed);
            }
        }
        // a possessive determiner cannot precede an article: "showed her the way"; a
        // demonstrative cannot either, nor a possessive or subject pronoun: after a verb
        // such a "that" is the complementizer ("think that the king …")
        for k in 0..n.saturating_sub(1) {
            let next_role = items[idxs[k + 1]].an.as_ref().and_then(|a| a.role);
            let next_article = matches!(next_role, Some(Role::DefArt | Role::IndefArt));
            if path[k] == Tag::Poss && (matches!(path[k + 1], Tag::Det | Tag::Poss) || next_article) && em[k][Tag::Pron.i()] > -6.0 {
                path[k] = Tag::Pron;
                if next_article {
                    path[k + 1] = Tag::Det;
                }
            }
            let next_nominal_start = next_article
                || matches!(next_role, Some(Role::PossDet(_)) | Some(Role::Pronoun { obj: false, .. }))
                || matches!(path[k + 1], Tag::Det | Tag::Poss);
            // the previous word may close an earlier (already tagged) segment: "Wiem, że …"
            let prev_tag = if k > 0 {
                Some(path[k - 1])
            } else {
                let mut j = idxs[0];
                let mut found = None;
                while j > 0 {
                    j -= 1;
                    if items[j].is_word() {
                        found = Some(items[j].tag);
                        break;
                    }
                    if items[j].kind == Kind::Punct && is_sentence_end(&items[j].text) {
                        break;
                    }
                }
                found
            };
            let after_verb = matches!(prev_tag, Some(Tag::V | Tag::Cop | Tag::Adj));
            let verb_follows = path[k + 1..].iter().any(|t| matches!(t, Tag::V | Tag::Cop | Tag::Aux));
            let that_like = items[idxs[k]]
                .an
                .as_ref()
                .map(|a| a.roles.iter().any(|(r, key, rel)| *rel >= 0.9 && (matches!(r, Role::Demonstrative { far: true, .. }) && key == "that" || *r == Role::Conj("sa"))))
                .unwrap_or(false);
            if that_like && matches!(path[k], Tag::Det | Tag::Pron | Tag::Prep | Tag::To | Tag::Wh) && em[k][Tag::Conj.i()] > -6.0 && after_verb && (next_nominal_start || path[k + 1] == Tag::V) && verb_follows {
                path[k] = Tag::Conj;
                items[idxs[k]].pin_role = Some((Role::Conj("sa"), "that".to_string()));
                if next_article {
                    path[k + 1] = Tag::Det;
                }
            }
            // "the man that I saw": after a noun the same word is the relative pronoun
            if that_like && matches!(path[k], Tag::Conj | Tag::Prep | Tag::To | Tag::Det | Tag::Pron) && prev_tag == Some(Tag::N) && em[k][Tag::Wh.i()] > -6.0 && verb_follows && (path[k] == Tag::Conj || next_nominal_start || path[k + 1] == Tag::V) {
                path[k] = Tag::Wh;
                items[idxs[k]].pin_role = Some((Role::Wh("i"), "that".to_string()));
            }
            // German/Dutch verb-final cluster: "dass er kommen wird" — the finite auxiliary
            // closes the clause after the infinitive/participle
            if k + 2 == n && path[k] == Tag::V && matches!(self.lang(), "de" | "nl" | "nds" | "lb" | "af") {
                let aux_next = items[idxs[k + 1]].an.as_ref().map(|a| a.roles.iter().any(|(r, _, rel)| matches!(r, Role::Aux(_)) && *rel >= 0.999)).unwrap_or(false);
                if aux_next && em[k + 1][Tag::Aux.i()] > -6.0 {
                    path[k + 1] = Tag::Aux;
                }
            }
        }
        for k in 0..n {
            items[idxs[k]].tag = path[k];
        }
        // role and features of the chosen reading
        for &i in idxs {
            let it = &items[i];
            let (role, role_key, tags) = match &it.an {
                None => (None, String::new(), 0),
                Some(a) => {
                    let cat = it.tag.cat();
                    let (role, key) = match it.pin_role.clone().or_else(|| role_for_tag(a, it.tag, !foreign)) {
                        Some((r, k)) => (Some(r), k),
                        None => (None, String::new()),
                    };
                    let tags = if matches!(it.tag, Tag::Cop | Tag::Aux | Tag::Det | Tag::Poss | Tag::Pron | Tag::Prep | Tag::To | Tag::Conj | Tag::Wh | Tag::Neg) && !key.is_empty() {
                        a.senses.iter().find(|s| s.key == key).map(|s| best_tagset(s, Cat::V, 0)).unwrap_or(0)
                    } else if let Some(c) = cand_for(a, cat) {
                        best_tagset(&a.senses[c.sense], cat, 0)
                    } else {
                        a.senses.first().map(|s| best_tagset(s, cat, 0)).unwrap_or(0)
                    };
                    (role, key, tags)
                }
            };
            let mut tags = tags;
            let forced = items[i].forced;
            if forced & f::CASES != 0 {
                tags = (tags & !f::CASES) | (forced & f::CASES);
            }
            tags |= forced & !f::CASES;
            items[i].role = role;
            items[i].role_key = role_key;
            items[i].tags = tags;
        }
    }
}

/// Best tag path for the emission scores.
fn viterbi(em: &[[f32; NTAG]], foreign: bool, closed: &[bool]) -> Vec<Tag> {
    let n = em.len();
    let mut score = vec![[f32::MIN; NTAG]; n];
    let mut back = vec![[0usize; NTAG]; n];
    for t in 0..NTAG {
        score[0][t] = start_score(ALL_TAGS[t]) + em[0][t];
    }
    for k in 1..n {
        let phrase_closed = closed.get(k - 1).copied().unwrap_or(false);
        for t in 0..NTAG {
            let mut best = (f32::MIN, 0usize);
            for p in 0..NTAG {
                let tr = if phrase_closed {
                    // after a case particle the next word begins a new phrase or is the predicate
                    match ALL_TAGS[t] {
                        Tag::V | Tag::Adj | Tag::Cop => 2.0,
                        Tag::N | Tag::Pron | Tag::Adv | Tag::Num | Tag::Det => 1.0,
                        _ => trans(ALL_TAGS[p], ALL_TAGS[t], foreign).min(0.0),
                    }
                } else {
                    trans(ALL_TAGS[p], ALL_TAGS[t], foreign)
                };
                let s = score[k - 1][p] + tr;
                if s > best.0 {
                    best = (s, p);
                }
            }
            score[k][t] = best.0 + em[k][t];
            back[k][t] = best.1;
        }
    }
    let mut best = (f32::MIN, 0usize);
    for t in 0..NTAG {
        let s = score[n - 1][t] + end_score(ALL_TAGS[t]);
        if s > best.0 {
            best = (s, t);
        }
    }
    let mut path = vec![Tag::N; n];
    let mut t = best.1;
    for k in (0..n).rev() {
        path[k] = ALL_TAGS[t];
        t = back[k][t];
    }
    path
}

fn cand_for(a: &Analysis, cat: Cat) -> Option<&Cand> {
    a.cands.iter().find(|c| c.cat == cat)
}

/// Tag emissions contributed by a grammatical role.
fn role_emissions(e: &mut [f32; NTAG], r: &Role, key: &str, english: bool) {
    use Tag::*;
    match r {
        Role::DefArt | Role::IndefArt => up(e, Det, 10.0),
        Role::Demonstrative { .. } => {
            up(e, Det, 7.0);
            up(e, Pron, 5.0);
            if key == "that" {
                up(e, Conj, 4.5);
                up(e, Wh, 4.0);
            }
        }
        Role::PossDet(_) => {
            up(e, Poss, 8.0);
            if key == "her" {
                up(e, Pron, 6.5);
            } else if matches!(key, "mine" | "yours" | "hers" | "ours" | "theirs" | "thine") {
                up(e, Pron, 7.0);
            }
        }
        Role::Pronoun { .. } => up(e, Pron, 9.0),
        Role::Copula => up(e, Cop, 9.5),
        Role::Aux(x) => {
            let (sa, sv) = match x {
                english::Aux::Perfect => (7.5, 7.0),
                english::Aux::Do => (9.0, 6.0),
                english::Aux::Let => (6.5, 6.0),
                english::Aux::Must if key == "need" => (5.5, 7.0),
                _ => (9.0, 2.0),
            };
            up(e, Aux, sa);
            up(e, V, sv);
            if matches!(key, "will" | "can" | "may" | "might" | "must") {
                up(e, N, 3.0);
            }
        }
        Role::Neg => up(e, Neg, if key == "not" { 12.0 } else { 10.0 }),
        Role::Prep(_) => {
            if key == "to" && english {
                up(e, To, 7.0);
                up(e, Prep, 6.0);
            } else if key == "to" {
                // a foreign "to" is first of all a preposition
                up(e, To, 6.0);
                up(e, Prep, 8.0);
            } else {
                up(e, Prep, 8.0);
            }
            if key == "as" {
                up(e, Conj, 4.5);
            }
            if key == "like" {
                up(e, V, 5.5);
            }
            if matches!(key, "before" | "after" | "until" | "till" | "since" | "while") {
                up(e, Conj, 5.0);
            }
            if matches!(key, "past" | "round" | "next" | "like" | "near") {
                up(e, Adj, 4.0);
            }
            if matches!(key, "off" | "on" | "in" | "out" | "up" | "down" | "over" | "about" | "around" | "along" | "through") {
                up(e, Adv, 4.5);
            }
        }
        Role::Conj(_) => {
            up(e, Conj, 9.0);
            if matches!(key, "so" | "then" | "yet" | "however" | "therefore" | "thus" | "hence") {
                up(e, Adv, 6.0);
            }
        }
        Role::Wh(_) => up(e, Wh, 9.0),
        Role::Number(_) => {
            up(e, Num, 10.0);
            if key == "one" {
                up(e, Pron, 4.5);
            }
        }
        Role::Ordinal(_) => {
            up(e, Adj, 8.5);
            if key == "second" {
                up(e, N, 4.0);
            }
        }
        Role::Quant(_) => {
            up(e, Det, 8.0);
            up(e, Pron, 4.5);
            if matches!(key, "much" | "more" | "most" | "little" | "else") {
                up(e, Adv, 4.5);
            }
            if key == "no" {
                up(e, Intj, 6.0);
            }
        }
        Role::There => {
            up(e, Pron, 7.0);
            up(e, Adv, 5.0);
        }
        Role::Word(_) => {
            if INTERJ_KEYS.contains(&key) {
                up(e, Intj, 9.0);
                up(e, Adv, 3.0);
            } else if PRONOUNISH_KEYS.contains(&key) {
                up(e, Pron, 8.0);
            } else {
                up(e, Adv, 8.0);
            }
        }
    }
}

/// Romance and German preposition + definite article contractions.
const FUSED_ARTICLES: &[&str] = &[
    "à", "às", "ao", "aos", "do", "da", "dos", "das", "no", "na", "nos", "nas", "pelo", "pela", "pelos", "pelas", "num", "numa",
    "al", "del", "au", "aux", "du", "des", "allo", "alla", "alle", "agli", "ai", "dello", "della", "delle", "degli", "dei", "dal",
    "dallo", "dalla", "dalle", "dagli", "dai", "nel", "nello", "nella", "nelle", "negli", "nei", "sul", "sullo", "sulla", "sulle",
    "sugli", "sui", "col", "coi", "pel", "als", "zum", "zur", "am", "im", "ans", "ins", "vom", "beim", "aufs", "fürs", "durchs",
    "ums", "übers", "unters", "hinters", "vors",
];

/// The interrogative an item stands for (chosen role, or its primary role for a
/// case-marked question word that was tagged as a noun: 誰が, 어디에).
fn wh_word(it: &Item) -> Option<&'static str> {
    match it.role() {
        Some(Role::Wh(wd)) => Some(wd),
        Some(Role::Pronoun { .. }) | Some(Role::Demonstrative { .. }) => None,
        _ => match it.an.as_ref().and_then(|a| a.role) {
            Some(Role::Wh(wd)) if it.forced != 0 || matches!(it.tag, Tag::N | Tag::Pron | Tag::Det) => Some(wd),
            _ => None,
        },
    }
}

/// The role (among an analysis' roles) that best supports the chosen tag.
fn role_for_tag(a: &Analysis, tag: Tag, english: bool) -> Option<(Role, String)> {
    let mut best: Option<(f32, Role, &String)> = None;
    for (r, key, rel) in &a.roles {
        let mut e = [-6.0f32; NTAG];
        role_emissions(&mut e, r, key, english);
        if e[tag.i()] > -6.0 {
            let mut v = e[tag.i()] * rel;
            // 이것 / これ: a firm demonstrative outranks its weaker "it" alias as a pronoun
            if tag == Tag::Pron && matches!(r, Role::Demonstrative { .. }) {
                v = 8.5 * rel;
            }
            if best.map(|(bv, _, _)| v > bv).unwrap_or(true) {
                best = Some((v, *r, key));
            }
        }
    }
    match best {
        Some((_, r, k)) => Some((r, k.clone())),
        None => a.roles.first().map(|(r, k, _)| (*r, k.clone())),
    }
}

// ------------------------------------------------------------------ phrase structure

#[derive(Default, Debug, Clone)]
struct NP {
    span: Vec<usize>,
    det: Option<usize>,
    def: bool,
    dem: Option<(bool, bool)>,
    poss: Option<(usize, Person)>,
    quants: Vec<usize>,
    /// numeral item and its value (None = keep digits)
    num: Option<(usize, Option<u64>)>,
    ords: Vec<usize>,
    adjs: Vec<usize>,
    /// possessors ("king's", genitive-tagged nouns)
    gens: Vec<usize>,
    /// post-nominal genitives ("król elfów", "regele elfilor")
    postgens: Vec<usize>,
    /// case-marked dependent nouns (source-language cases)
    deps: Vec<(usize, Case)>,
    nouns: Vec<usize>,
    pron: Option<usize>,
    there: bool,
    plural: bool,
}

impl NP {
    fn is_empty(&self) -> bool {
        self.span.is_empty()
    }
    fn head(&self) -> Option<usize> {
        self.nouns.last().copied()
    }
    /// A genitive dependent whose form is also a dative (Polish "królowej", Romanian
    /// "reginei"): after a verb of giving it is the recipient, not a possessor.
    fn dative_dependent(&self, items: &[Item]) -> Option<usize> {
        self.postgens.iter().chain(self.gens.iter()).copied().find(|&g| noun_cases(&items[g]) & f::DAT != 0)
    }
    fn person(&self, items: &[Item]) -> Option<Person> {
        let p = self.pron?;
        match items[p].role() {
            Some(Role::Pronoun { p, .. }) => Some(p),
            Some(Role::PossDet(p)) => Some(p),
            _ => None,
        }
    }
    fn number(&self) -> Number {
        if self.plural {
            Number::Pl
        } else {
            Number::Sg
        }
    }
}

#[derive(Default, Debug, Clone)]
struct VP {
    span: Vec<usize>,
    to: Option<usize>,
    auxs: Vec<(usize, Aux)>,
    cops: Vec<usize>,
    neg: Vec<usize>,
    advs_pre: Vec<usize>,
    advs_post: Vec<usize>,
    verb: Option<usize>,
}

impl VP {
    fn has_head(&self) -> bool {
        self.verb.is_some() || !self.cops.is_empty() || !self.auxs.is_empty()
    }
}

#[derive(Debug, Clone)]
struct PP {
    prep: usize,
    role: PrepRole,
    np: Option<NP>,
}

#[derive(Debug, Clone)]
enum Node {
    NP(NP),
    VP(VP),
    PP(PP),
    Adj(usize),
    Adv(usize),
    Conj(usize),
    Wh(usize),
    Intj(usize),
    Other(usize),
}

fn starts_np(t: Tag) -> bool {
    matches!(t, Tag::Det | Tag::Poss | Tag::Num | Tag::Adj | Tag::N | Tag::Pron)
}

fn is_genitive_marked(it: &Item) -> bool {
    let t = it.text.as_str();
    t.ends_with("'s") || t.ends_with("’s") || t.ends_with("s'") || t.ends_with("s’") || it.tags & f::GEN != 0
}

/// Case bits forced on a phrase by a particle (Japanese/Korean/Chinese), 0 if none.
fn np_forced(items: &[Item], np: &NP) -> u64 {
    np.head().or(np.pron).map(|h| items[h].forced & f::CASES).unwrap_or(0)
}

/// A phrase whose determiner or pronoun is case-marked and cannot be nominative
/// ("den König", "ihm"): never the subject.
fn np_oblique_only(items: &[Item], np: &NP, vtags: u64, other_subject: bool) -> bool {
    let Some(d) = np.det.or(np.pron).or_else(|| np.head()) else { return false };
    if np.det.is_none() && np.pron.is_none() {
        // bare noun ("króla", "королю"): all inflectional readings of the chosen lemma;
        // inflection tables are incomplete (звёзды lacks its nominative plural), so the
        // reading counts only when another subject exists or the noun cannot agree with
        // the verb
        let c = noun_cases(&items[d]);
        if c == 0 || c & f::NOM != 0 {
            return false;
        }
        if other_subject || (vtags & (f::P1 | f::P2) != 0 && vtags & f::P3 == 0) {
            return true;
        }
        let noun_pl = items[d].tags & f::PL != 0;
        return vtags & f::NUMBER != 0 && ((noun_pl && vtags & f::PL == 0) || (!noun_pl && vtags & f::SG == 0));
    }
    let it = &items[d];
    let Some(a) = &it.an else { return false };
    let key = &it.role_key;
    let cases = a
        .senses
        .iter()
        .filter(|s| &s.key == key)
        .flat_map(|s| s.tagsets.iter())
        .fold(0u64, |acc, t| acc | (t & f::CASES));
    cases != 0 && cases & f::NOM == 0
}

/// Union of the case readings of a noun's chosen sense (all homographic inflections).
fn noun_cases(it: &Item) -> u64 {
    let Some(a) = &it.an else { return it.tags & f::CASES };
    let Some(c) = cand_for(a, Cat::N) else { return it.tags & f::CASES };
    let verbal = f::TENSES | f::MOODS | f::PERSONS;
    let u = a.senses[c.sense].tagsets.iter().filter(|&&t| t & verbal == 0).fold(0, |acc, &t| acc | (t & f::CASES));
    if u == 0 { it.tags & f::CASES } else { u }
}

/// A noun form that can only be a genitive (never a nominative) in the source language.
fn genitive_only(it: &Item) -> bool {
    let c = noun_cases(it);
    c & f::GEN != 0 && c & f::NOM == 0 && !(it.text.ends_with("'s") || it.text.ends_with("’s"))
}

/// Union of the case/number readings a determiner allows.
fn det_constraints(it: &Item) -> u64 {
    let Some(a) = &it.an else { return 0 };
    let key = it.role_key();
    let mut acc = 0u64;
    for s in a.senses.iter().filter(|s| s.key == key) {
        for &t in &s.tagsets {
            acc |= t & (f::CASES | f::NUMBER);
        }
    }
    acc
}

/// Re-select the head noun's tagset given the determiner's constraints.
fn refine_noun_tags(items: &mut [Item], head: usize, constraints: u64) {
    let it = &items[head];
    let Some(a) = &it.an else { return };
    let Some(c) = cand_for(a, Cat::N) else { return };
    let tags = best_tagset(&a.senses[c.sense], Cat::N, constraints);
    items[head].tags = tags;
}

fn parse_np(items: &[Item], idxs: &[usize], mut p: usize) -> (NP, usize) {
    let mut np = NP::default();
    while p < idxs.len() {
        let k = idxs[p];
        let it = &items[k];
        let next_tag = idxs.get(p + 1).map(|&n| items[n].tag);
        match it.tag {
            Tag::Det if np.nouns.is_empty() && np.adjs.is_empty() && np.pron.is_none() => match it.role() {
                Some(Role::Quant(_)) => np.quants.push(k),
                Some(Role::DefArt) if np.det.is_none() => {
                    np.det = Some(k);
                    np.def = true;
                }
                Some(Role::IndefArt) if np.det.is_none() => np.det = Some(k),
                Some(Role::Demonstrative { far, plural }) if np.det.is_none() => {
                    np.det = Some(k);
                    np.dem = Some((far, plural));
                    if plural {
                        np.plural = true;
                    }
                }
                Some(_) if np.det.is_none() => np.det = Some(k),
                _ => {
                    if np.is_empty() {
                        np.quants.push(k);
                    } else {
                        break;
                    }
                }
            },
            Tag::Poss if np.nouns.is_empty() && np.pron.is_none() && np.poss.is_none() => match it.role() {
                Some(Role::PossDet(person)) => np.poss = Some((k, person)),
                _ => {
                    if np.is_empty() {
                        np.gens.push(k);
                    } else {
                        break;
                    }
                }
            },
            Tag::Num if np.nouns.is_empty() && np.pron.is_none() => {
                if np.num.is_some() {
                    break;
                }
                let val = if it.kind == Kind::Number {
                    None
                } else if let Some(Role::Number(n)) = it.role() {
                    Some(n)
                } else {
                    Some(0)
                };
                np.num = Some((k, val));
                if matches!(val, Some(n) if n >= 3) || (val.is_none() && it.text.trim_start_matches('0') != "1" && it.text != "1") {
                    np.plural = true;
                }
            }
            Tag::Adj if np.nouns.is_empty() && np.pron.is_none() => {
                if matches!(it.role(), Some(Role::Ordinal(_))) {
                    np.ords.push(k);
                } else {
                    np.adjs.push(k);
                }
            }
            Tag::N if np.pron.is_none() && it.phrase_end => {
                // a particle closed the phrase right after this noun: it is the head
                np.nouns.push(k);
                if it.tags & f::PL != 0 {
                    np.plural = true;
                }
                np.span.push(k);
                p += 1;
                break;
            }
            Tag::N if np.pron.is_none() => {
                // a following noun that can only be oblique (Finnish "kuningattarelle") starts
                // its own phrase; it cannot be the head this genitive depends on
                let next_headlike = match next_tag {
                    Some(Tag::N) => {
                        let nx = &items[idxs[p + 1]];
                        let nc = noun_cases(nx);
                        nx.forced != 0 || nc == 0 || nc & (f::NOM | f::ACC | f::GEN) != 0
                    }
                    _ => true,
                };
                let followed_by_nominal = matches!(next_tag, Some(Tag::N) | Some(Tag::Adj) | Some(Tag::Num)) && next_headlike;
                let next_gen = matches!(next_tag, Some(Tag::N)) && genitive_only(&items[idxs[p + 1]]) && items[idxs[p + 1]].forced == 0;
                let bare_gen = genitive_only(it) && it.forced == 0;
                if !np.nouns.is_empty() && bare_gen {
                    // post-nominal genitive dependent of an already-seen head
                    np.postgens.push(k);
                    np.span.push(k);
                    p += 1;
                    if next_gen {
                        continue;
                    }
                    break;
                } else if next_gen {
                    // head followed by its genitive: "król elfów"
                    np.nouns.push(k);
                    if it.tags & f::PL != 0 {
                        np.plural = true;
                    }
                    if it.tags & f::DEF != 0 && np.det.is_none() && np.poss.is_none() {
                        np.def = true;
                    }
                } else if is_genitive_marked(it) && followed_by_nominal {
                    np.gens.push(k);
                } else if let Some(c) = case_of_tags(it.tags).filter(|_| matches!(next_tag, Some(Tag::N)) && next_headlike) {
                    np.deps.push((k, c));
                } else {
                    np.nouns.push(k);
                    if it.tags & f::PL != 0 {
                        np.plural = true;
                    }
                    // suffixed definite article (Scandinavian, Romanian, Bulgarian, Albanian)
                    if it.tags & f::DEF != 0 && np.det.is_none() && np.poss.is_none() {
                        np.def = true;
                    }
                    if !matches!(next_tag, Some(Tag::N)) || it.phrase_end {
                        np.span.push(k);
                        p += 1;
                        break;
                    }
                }
            }
            Tag::Pron if np.is_empty() && it.forced & f::GEN != 0 && matches!(next_tag, Some(Tag::N) | Some(Tag::Adj) | Some(Tag::Num)) => {
                match it.role() {
                    Some(Role::Pronoun { p: person, .. }) => np.poss = Some((k, person)),
                    _ => np.gens.push(k),
                }
            }
            Tag::Pron if np.is_empty() => {
                np.pron = Some(k);
                match it.role() {
                    Some(Role::Pronoun { p: person, .. }) => np.plural = person.is_plural(),
                    Some(Role::There) => np.there = true,
                    Some(Role::Demonstrative { plural, .. }) => np.plural = plural,
                    _ => np.plural = it.tags & f::PL != 0,
                }
                np.span.push(k);
                p += 1;
                break;
            }
            _ => break,
        }
        np.span.push(k);
        p += 1;
    }
    (np, p)
}

fn parse_vp(items: &[Item], idxs: &[usize], mut p: usize, english: bool) -> (VP, usize) {
    let mut vp = VP::default();
    if items[idxs[p]].tag == Tag::To {
        vp.to = Some(idxs[p]);
        vp.span.push(idxs[p]);
        p += 1;
    }
    while p < idxs.len() {
        let k = idxs[p];
        let it = &items[k];
        match it.tag {
            Tag::Aux if vp.verb.is_none() => match it.role() {
                Some(Role::Aux(a)) => vp.auxs.push((k, a)),
                _ => {
                    vp.verb = Some(k);
                }
            },
            // verb-final auxiliary ("kommen wird")
            Tag::Aux if !english && matches!(it.role(), Some(Role::Aux(_))) => {
                if let Some(Role::Aux(a)) = it.role() {
                    vp.auxs.push((k, a));
                }
            }
            Tag::Cop if vp.verb.is_none() => vp.cops.push(k),
            Tag::Neg if vp.verb.is_none() => vp.neg.push(k),
            Tag::Adv => {
                if vp.verb.is_none() {
                    vp.advs_pre.push(k)
                } else {
                    vp.advs_post.push(k)
                }
            }
            Tag::V if vp.verb.is_none() => vp.verb = Some(k),
            _ => break,
        }
        vp.span.push(k);
        p += 1;
    }
    (vp, p)
}

fn parse(items: &[Item], idxs: &[usize], english: bool) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut p = 0;
    while p < idxs.len() {
        let k = idxs[p];
        let t = items[k].tag;
        let next = idxs.get(p + 1).map(|&n| items[n].tag);
        match t {
            _ if starts_np(t) => {
                let (np, np_end) = parse_np(items, idxs, p);
                if np.is_empty() {
                    nodes.push(Node::Other(k));
                    p += 1;
                } else {
                    nodes.push(Node::NP(np));
                    p = np_end;
                }
            }
            Tag::Prep => {
                let role = match items[k].role() {
                    Some(Role::Prep(r)) => r,
                    _ => PrepRole::Case(Case::Nom),
                };
                // contracted preposition + article ("del", "zum", "au")
                let low = items[k].text.to_lowercase();
                let fused_article = items[k].an.as_ref().map(|a| a.roles.iter().any(|(r, _, _)| matches!(r, Role::DefArt))).unwrap_or(false)
                    || (!english && FUSED_ARTICLES.contains(&low.as_str()));
                p += 1;
                let np = if p < idxs.len() && starts_np(items[idxs[p]].tag) {
                    let (mut np, e) = parse_np(items, idxs, p);
                    if fused_article && np.det.is_none() && np.pron.is_none() {
                        np.def = true;
                    }
                    p = e;
                    Some(np)
                } else {
                    None
                };
                nodes.push(Node::PP(PP { prep: k, role, np }));
            }
            Tag::To => {
                let verbal = matches!(next, Some(Tag::V) | Some(Tag::Aux) | Some(Tag::Cop) | Some(Tag::Neg)) || (next == Some(Tag::Adv) && idxs.get(p + 2).map(|&n| items[n].tag) == Some(Tag::V));
                if verbal {
                    let (vp, e) = parse_vp(items, idxs, p, english);
                    nodes.push(Node::VP(vp));
                    p = e;
                } else {
                    p += 1;
                    let np = if p < idxs.len() && starts_np(items[idxs[p]].tag) {
                        let (np, e) = parse_np(items, idxs, p);
                        p = e;
                        Some(np)
                    } else {
                        None
                    };
                    nodes.push(Node::PP(PP { prep: k, role: PrepRole::Case(Case::All), np }));
                }
            }
            Tag::Aux | Tag::Cop | Tag::Neg | Tag::V => {
                let (vp, e) = parse_vp(items, idxs, p, english);
                nodes.push(Node::VP(vp));
                p = e;
            }
            Tag::Adv => {
                // adverb directly before a verb group belongs to it
                if matches!(next, Some(Tag::V) | Some(Tag::Aux) | Some(Tag::Cop) | Some(Tag::Neg)) {
                    let (vp, e) = parse_vp(items, idxs, p, english);
                    nodes.push(Node::VP(vp));
                    p = e;
                } else {
                    nodes.push(Node::Adv(k));
                    p += 1;
                }
            }
            Tag::Adj => {
                nodes.push(Node::Adj(k));
                p += 1;
            }
            Tag::Conj => {
                nodes.push(Node::Conj(k));
                p += 1;
            }
            Tag::Wh => {
                nodes.push(Node::Wh(k));
                p += 1;
            }
            Tag::Intj => {
                nodes.push(Node::Intj(k));
                p += 1;
            }
            _ => {
                nodes.push(Node::Other(k));
                p += 1;
            }
        }
    }
    nodes
}

// ------------------------------------------------------------------ rendering

#[derive(Clone, Debug)]
struct Word {
    text: String,
    src: Option<usize>,
    name: bool,
    /// Source text kept verbatim: no case adjustments.
    keep: bool,
}

fn w(text: impl Into<String>, src: usize) -> Word {
    Word { text: text.into(), src: Some(src), name: false, keep: false }
}

fn free(text: impl Into<String>) -> Word {
    Word { text: text.into(), src: None, name: false, keep: false }
}

/// Preposition carrying a case for a word that cannot take a case ending (a name in its own
/// script, a code span, a URL).
fn case_prep(case: Case) -> Option<&'static str> {
    match case {
        Case::All => Some("na"),
        Case::Loc => Some("mi"),
        Case::Abl => Some("ho"),
        Case::Ins => Some("ó"),
        Case::Dat => Some("an"),
        Case::Gen | Case::Poss => Some("o"),
        Case::Nom => None,
    }
}

/// Attach words rendered without a source index to the phrase's anchor word, so protected
/// tokens can be placed relative to them and source capitalisation carries over.
fn stamp(mut ws: Vec<Word>, anchor: Option<usize>) -> Vec<Word> {
    if let Some(k) = anchor {
        for wd in ws.iter_mut() {
            if wd.src.is_none() {
                wd.src = Some(k);
            }
        }
    }
    ws
}

/// The next word after item `k` is a protected token (code span, URL).
fn protected_follows(items: &[Item], k: usize) -> bool {
    items[k + 1..].iter().find(|it| it.kind != Kind::Space).map(|it| it.protected).unwrap_or(false)
}

fn synthetic_verb(form: &'static str, gloss: &'static str) -> Entry {
    Entry { id: u32::MAX, form, stem: "", pos: pos::VERB, lang: elang::NQ, vclass: vclass::BASIC, mark: 0, refs: 0, gloss, infl: Vec::new(), tengwar: "", speech: "" }
}

fn clean_form(s: &str) -> String {
    quenya::respell(&quenya::strip_diaeresis(s.trim_end_matches('-').trim_start_matches('-')))
}

/// Adapt a foreign word to Quenya phonology (used for untranslatable names with --adapt).
fn adapt(word: &str) -> String {
    let lower = word.to_lowercase();
    let mut s = lower
        .replace("th", "s")
        .replace("sh", "s")
        .replace("ch", "c")
        .replace("ph", "f")
        .replace("ck", "c")
        .replace("qu", "qu")
        .replace('k', "c")
        .replace('x', "cs")
        .replace('z', "s")
        .replace('j', "y")
        .replace('w', "v")
        .replace("ii", "i")
        .replace("oo", "ó")
        .replace("ee", "í");
    if s.starts_with('b') {
        s.replace_range(0..1, "v");
    }
    if let Some(last) = s.chars().last() {
        if last.is_alphabetic() && !quenya::is_vowel(last) && !matches!(last, 'n' | 'l' | 'r' | 's' | 't') {
            s.push('ë');
        }
    }
    if word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
        s = title_case(&s);
    }
    s
}

impl<'d> Translator<'d> {
    fn unknown_word(&self, it: &Item, number: Number, case: Case, poss: Option<Person>) -> Word {
        let mut text = it.text.clone();
        if self.opts.adapt && !it.protected {
            text = adapt(&text);
        }
        let latin = text.chars().all(|c| c.is_ascii() || matches!(c, 'À'..='ɏ' | 'Ḁ'..='ỿ'));
        if !it.protected && latin && (case != Case::Nom || poss.is_some() || number == Number::Pl) {
            text = quenya::inflect_noun(&text, "", number, case, poss);
        } else if !it.protected && !latin {
            // 東京に: a name kept in its own script cannot take a case ending; a preposition
            // carries the case instead
            if let Some(p) = case_prep(case) {
                text = format!("{} {}", p, text);
            }
        }
        if self.opts.mark_unknown && !it.protected {
            text = format!("⟦{}⟧", text);
        }
        let prefixed = !latin && text.contains(' ');
        Word { text, src: Some(it.idx), name: it.cap == Cap::Title && !prefixed, keep: true }
    }

    fn cand<'a>(&self, it: &'a Item, cat: Cat) -> Option<&'a Cand> {
        it.an.as_ref().and_then(|a| cand_for(a, cat))
    }

    fn best_cand<'a>(&self, it: &'a Item) -> Option<&'a Cand> {
        it.an.as_ref().and_then(|a| a.cands.first())
    }

    fn entry_word(&self, it: &Item, e: &Entry, text: String) -> Word {
        Word { text, src: Some(it.idx), name: e.pos == pos::NAME, keep: false }
    }

    /// Look up a Quenya verb entry by English key (e.g. "can" → pol-).
    fn verb_by_en(&mut self, keys: &[&str], prefix: &str, fallback: Entry) -> Entry {
        for k in keys {
            let a = self.an.analyze_en(k);
            if let Some(c) = a.cands.iter().find(|c| c.cat == Cat::V && c.entry.form.starts_with(prefix)) {
                return c.entry.clone();
            }
        }
        fallback
    }

    // ---- nouns / adjectives

    fn render_noun_item(&self, it: &Item, number: Number, case: Case, poss: Option<Person>) -> Word {
        let mut wd = self.render_noun_item_inner(it, number, case, poss);
        if wd.src.is_none() {
            wd.src = Some(it.idx);
        }
        wd
    }

    fn render_noun_item_inner(&self, it: &Item, number: Number, case: Case, poss: Option<Person>) -> Word {
        if let Some(a) = &it.an {
            if let Some(Role::Word(wd)) = a.role {
                if it.tag == Tag::N || it.tag == Tag::Pron {
                    if a.cands.iter().all(|c| c.cat != Cat::N) {
                        return free(quenya::inflect_noun(wd, "", number, case, poss));
                    }
                }
            }
        }
        if let Some(c) = self.cand(it, Cat::N) {
            let e = &c.entry;
            let mut wd = self.entry_word(it, e, quenya::noun(e, number, case, poss));
            wd.name = e.pos == pos::NAME || it.is_name;
            return wd;
        }
        if let Some(c) = self.cand(it, Cat::V) {
            let g = quenya::gerund(&c.entry);
            return free(quenya::inflect_noun(&g, "", number, case, poss));
        }
        if let Some(c) = self.cand(it, Cat::Adj) {
            let a = clean_form(c.entry.form);
            return free(quenya::inflect_noun(&a, "", number, case, poss));
        }
        if let Some(c) = self.best_cand(it) {
            let a = clean_form(c.entry.form);
            return free(quenya::inflect_noun(&a, "", number, case, poss));
        }
        self.unknown_word(it, number, case, poss)
    }

    fn render_adj_item(&self, it: &Item, number: Number) -> Word {
        let mut wd = self.render_adj_item_inner(it, number);
        if wd.src.is_none() {
            wd.src = Some(it.idx);
        }
        wd
    }

    fn render_adj_item_inner(&self, it: &Item, number: Number) -> Word {
        if let Some(a) = &it.an {
            if let Some(Role::Quant(q)) = a.role {
                let s = if number == Number::Pl { quenya::adjective_plural(q) } else { q.to_string() };
                return free(s);
            }
            if let Some(Role::Ordinal(n)) = a.role {
                if let Some(o) = quenya::ordinal(n) {
                    let s = if number == Number::Pl { quenya::adjective_plural(o) } else { o.to_string() };
                    return free(s);
                }
            }
            if let Some(Role::Word(wd)) = a.role {
                if a.cands.iter().all(|c| c.cat != Cat::Adj) {
                    return free(wd);
                }
            }
        }
        if let Some(c) = self.cand(it, Cat::Adj) {
            let mut s = quenya::adjective(&c.entry, number);
            if it.tags & f::SUP != 0 {
                s = quenya::intensify(&quenya::adjective(&c.entry, Number::Sg), true);
                if number == Number::Pl {
                    s = quenya::adjective_plural(&s);
                }
            }
            return free(s);
        }
        if let Some(c) = self.cand(it, Cat::V) {
            let p = if it.tags & (f::PASS | f::PAST) != 0 || it.text.ends_with("ed") || it.text.ends_with("en") { quenya::passive_participle(&c.entry) } else { quenya::active_participle(&c.entry) };
            return free(if number == Number::Pl { quenya::adjective_plural(&p) } else { p });
        }
        if let Some(c) = self.cand(it, Cat::N) {
            return free(quenya::noun(&c.entry, Number::Sg, Case::Nom, None));
        }
        if let Some(c) = self.best_cand(it) {
            return free(clean_form(c.entry.form));
        }
        self.unknown_word(it, Number::Sg, Case::Nom, None)
    }

    fn render_adv_item(&self, it: &Item) -> Word {
        let mut wd = self.render_adv_item_inner(it);
        if wd.src.is_none() {
            wd.src = Some(it.idx);
        }
        wd
    }

    fn render_adv_item_inner(&self, it: &Item) -> Word {
        if let Some(a) = &it.an {
            match a.role {
                Some(Role::Word(wd)) => return free(wd),
                Some(Role::There) => return free("tanomë"),
                Some(Role::Conj(c)) => return free(c),
                Some(Role::Quant(q)) => return free(quenya::adverb(q)),
                Some(Role::Neg) => return free("lá"),
                Some(Role::Prep(PrepRole::Word(p))) if self.cand(it, Cat::Adv).is_none() => return free(p),
                _ => {}
            }
        }
        if let Some(c) = self.cand(it, Cat::Adv) {
            return free(clean_form(c.entry.form));
        }
        if let Some(c) = self.cand(it, Cat::Adj) {
            return free(quenya::adverb(&quenya::adjective(&c.entry, Number::Sg)));
        }
        if let Some(c) = self.best_cand(it) {
            return free(clean_form(c.entry.form));
        }
        self.unknown_word(it, Number::Sg, Case::Nom, None)
    }

    fn render_pron(&self, it: &Item, case: Case, plural_hint: bool) -> Word {
        let a = it.an.as_ref();
        // the role chosen for the item's tag ("la" tagged Pron is "her", not the article)
        let role = match it.role() {
            Some(Role::Pronoun { .. } | Role::Demonstrative { .. } | Role::Wh(_) | Role::Quant(_) | Role::Number(_)) => it.role(),
            _ => a.and_then(|a| a.role),
        };
        match role {
            Some(Role::Pronoun { p, refl, .. }) => {
                if refl {
                    return free(p.reflexive());
                }
                free(pron_case(p, case))
            }
            Some(Role::PossDet(p)) => free(format!("{}nya", p.independent())),
            Some(Role::Demonstrative { far, plural }) => {
                let base = if far { "tana" } else { "sina" };
                let n = if plural || plural_hint { Number::Pl } else { Number::Sg };
                free(quenya::inflect_noun(base, "", n, case, None))
            }
            Some(Role::Number(1)) => free(quenya::inflect_noun("quén", "", Number::Sg, case, None)),
            Some(Role::Quant(q)) => {
                let n = if plural_hint || matches!(it.role_key(), "all" | "many" | "some" | "few" | "both" | "several") { Number::Pl } else { Number::Sg };
                free(quenya::inflect_noun(q, "", n, case, None))
            }
            Some(Role::Word(wd)) => free(quenya::inflect_noun(wd, "", Number::Sg, case, None)),
            Some(Role::There) => free("tanomë"),
            Some(Role::Wh(wd)) => free(wd),
            _ => {
                if let Some(c) = self.cand(it, Cat::Pron) {
                    return free(quenya::inflect_noun(&clean_form(c.entry.form), "", if plural_hint { Number::Pl } else { Number::Sg }, case, None));
                }
                self.render_noun_item(it, if plural_hint { Number::Pl } else { Number::Sg }, case, None)
            }
        }
    }

    fn render_np(&self, items: &[Item], np: &NP, case: Case) -> Vec<Word> {
        self.render_np_ex(items, np, case, true)
    }

    /// `src_case`: let the source language's case marking on a bare NP decide the Quenya case.
    fn render_np_ex(&self, items: &[Item], np: &NP, case: Case, src_case: bool) -> Vec<Word> {
        let ws = self.render_np_ex_inner(items, np, case, src_case);
        stamp(ws, np.head().or(np.pron).or(np.det).or_else(|| np.span.first().copied()))
    }

    fn render_np_ex_inner(&self, items: &[Item], np: &NP, case: Case, src_case: bool) -> Vec<Word> {
        let mut out: Vec<Word> = Vec::new();
        // 誰が / 어디에: a question word carrying a case particle
        if np.span.len() == 1 {
            if let Some(wd) = wh_word(&items[np.span[0]]) {
                let wd = match (wd, case) {
                    ("man", Case::Gen | Case::Poss) => "mano",
                    ("man", Case::Dat) => "manen",
                    ("mana", Case::Loc) => "massë",
                    _ => wd,
                };
                out.push(w(wd, np.span[0]));
                return out;
            }
        }
        if let Some(p) = np.pron {
            out.push(self.render_pron(&items[p], case, np.plural));
            return out;
        }
        let number = np.number();
        let head = np.head();
        // the numeral decides the head's number in Quenya
        let numval = np.num.and_then(|(_, v)| v);
        let head_number = match numval {
            Some(2) => Number::Sg,
            Some(n) if n >= 3 => Number::Pl,
            _ => number,
        };
        let head_is_name = head.map(|h| self.cand(&items[h], Cat::N).map(|c| c.entry.pos == pos::NAME).unwrap_or(items[h].cap == Cap::Title && items[h].an.as_ref().map(|a| a.is_empty()).unwrap_or(true))).unwrap_or(false);
        let effective_case = if case == Case::Nom && src_case { head.and_then(|h| case_of_tags(items[h].tags)).unwrap_or(Case::Nom) } else { case };
        let poss = np.poss.map(|(_, p)| p);
        if np.def && poss.is_none() && np.dem.is_none() && !head_is_name && head.is_some() {
            out.push(w("i", np.det.unwrap_or(np.span[0])));
        }
        if let Some((far, plural)) = np.dem {
            let base = if far { "tana" } else { "sina" };
            out.push(free(if plural || number == Number::Pl { quenya::adjective_plural(base) } else { base.to_string() }));
        }
        match np.num {
            Some((k, Some(1))) if head.is_some() => out.push(w("min", k)),
            Some((k, None)) => {
                let txt = &items[k].text;
                let digits: String = txt.chars().filter(|c| c.is_ascii_digit()).collect();
                let parsed = if digits.len() == txt.len() { digits.parse::<u64>().ok() } else { None };
                match parsed.filter(|_| self.opts.numerals).and_then(quenya::cardinal) {
                    Some(c) if head.is_some() => {
                        // numerals ≥2 follow the noun; handled below
                        let _ = c;
                    }
                    Some(c) => out.push(w(c, k)),
                    None => out.push(w(txt.clone(), k)),
                }
            }
            Some((k, Some(n))) if head.is_none() => {
                out.push(w(quenya::cardinal(n).unwrap_or_else(|| n.to_string()), k));
            }
            _ => {}
        }
        for &o in &np.ords {
            out.push(self.render_adj_item(&items[o], head_number));
        }
        for &q in &np.quants {
            match wh_word(&items[q]) {
                Some(wd) => out.push(w(wd, q)),
                None => out.push(self.render_adj_item(&items[q], head_number)),
            }
        }
        for &a in &np.adjs {
            if let Some(wd) = wh_word(&items[a]) {
                out.push(w(wd, a));
                continue;
            }
            let mut wd = self.render_adj_item(&items[a], head_number);
            if head.is_none() && np.adjs.last() == Some(&a) {
                // adjective used as a noun
                wd = self.render_noun_item(&items[a], head_number, effective_case, poss);
                if let Some(c) = self.cand(&items[a], Cat::Adj) {
                    wd = free(quenya::inflect_noun(&quenya::adjective(&c.entry, Number::Sg), "", head_number, effective_case, poss));
                }
            }
            out.push(wd);
        }
        for &g in &np.gens {
            let gnum = if items[g].tags & f::PL != 0 || items[g].text.ends_with("s'") || items[g].text.ends_with("s’") { Number::Pl } else { Number::Sg };
            out.push(self.render_noun_item(&items[g], gnum, Case::Poss, None));
        }
        for &(d, c) in &np.deps {
            let dnum = if items[d].tags & f::PL != 0 { Number::Pl } else { Number::Sg };
            out.push(self.render_noun_item(&items[d], dnum, c, None));
        }
        let mut postgens: Vec<Word> = Vec::new();
        for &g in &np.postgens {
            let gnum = if items[g].tags & f::PL != 0 { Number::Pl } else { Number::Sg };
            postgens.push(self.render_noun_item(&items[g], gnum, Case::Gen, None));
        }
        if let Some(h) = head {
            for &n in &np.nouns {
                if n != h {
                    let nn = if items[n].tags & f::PL != 0 { Number::Pl } else { Number::Sg };
                    out.push(self.render_noun_item(&items[n], nn, Case::Nom, None));
                }
            }
            let (hc, hposs) = match numval {
                Some(2) => (Case::Nom, poss),
                _ => (effective_case, poss),
            };
            out.push(self.render_noun_item(&items[h], head_number, hc, hposs));
            match np.num {
                Some((k, Some(2))) => out.push(w(quenya::inflect_noun("atta", "", Number::Sg, effective_case, None), k)),
                Some((k, Some(n))) if n >= 3 => out.push(w(quenya::cardinal(n).unwrap_or_else(|| n.to_string()), k)),
                Some((k, None)) if self.opts.numerals => {
                    let digits: String = items[k].text.chars().filter(|c| c.is_ascii_digit()).collect();
                    if let Some(c) = digits.parse::<u64>().ok().and_then(quenya::cardinal) {
                        if digits.len() == items[k].text.len() && digits != "1" {
                            out.push(w(c, k));
                        }
                    }
                }
                _ => {}
            }
        }
        out.extend(postgens);
        if out.is_empty() && np.def && np.span.len() == 1 {
            // the article of a phrase whose noun is a protected token ("the `/lsp` command")
            if let Some(pw) = case_prep(effective_case) {
                out.push(free(pw));
            }
            out.push(w("i", np.span[0]));
        }
        if out.is_empty() {
            for &k in &np.span {
                out.push(self.render_noun_item(&items[k], number, effective_case, poss));
            }
        }
        out
    }

    fn render_pp(&self, items: &[Item], pp: &PP, verb_key: &str) -> Vec<Word> {
        let ws = self.render_pp_inner(items, pp, verb_key);
        stamp(ws, Some(pp.prep))
    }

    fn render_pp_inner(&self, items: &[Item], pp: &PP, verb_key: &str) -> Vec<Word> {
        let mut out = Vec::new();
        let prep = &items[pp.prep];
        match (&pp.np, pp.role) {
            (Some(np), PrepRole::Case(c)) => {
                let mut c = c;
                // a preposition that can also mean "to" (Dutch "aan") marks the recipient of giving
                let to_like = prep
                    .an
                    .as_ref()
                    .map(|a| a.roles.iter().any(|(r, k, rel)| matches!(r, Role::Prep(PrepRole::Case(Case::All | Case::Dat))) && matches!(k.as_str(), "to" | "for") && *rel >= 0.85))
                    .unwrap_or(false);
                let giving = GIVING_VERBS.contains(&verb_key);
                if (c == Case::All && (np.pron.is_some() || giving)) || (to_like && giving && matches!(c, Case::Loc | Case::All)) {
                    c = Case::Dat;
                }
                out.extend(self.render_np(items, np, c));
            }
            (Some(np), PrepRole::Word(pw)) => {
                out.push(w(pw, pp.prep));
                out.extend(self.render_np_ex(items, np, Case::Nom, false));
            }
            (Some(np), PrepRole::WordGen(pw)) => {
                out.push(w(pw, pp.prep));
                out.extend(self.render_np(items, np, Case::Gen));
            }
            (Some(np), PrepRole::With) => {
                let animate = np.pron.is_some() || np.head().map(|h| self.cand(&items[h], Cat::N).map(|c| c.entry.pos == pos::NAME).unwrap_or(items[h].cap == Cap::Title)).unwrap_or(false);
                if animate {
                    out.push(w("as", pp.prep));
                    out.extend(self.render_np_ex(items, np, Case::Nom, false));
                } else {
                    out.extend(self.render_np(items, np, Case::Ins));
                }
            }
            (Some(np), PrepRole::Than) => {
                out.push(w("lá", pp.prep));
                out.extend(self.render_np_ex(items, np, Case::Nom, false));
            }
            (None, PrepRole::Word(pw)) | (None, PrepRole::WordGen(pw)) => out.push(w(pw, pp.prep)),
            (None, PrepRole::Than) => out.push(w("lá", pp.prep)),
            (None, PrepRole::With) => out.push(w("as", pp.prep)),
            (None, PrepRole::Case(c)) if protected_follows(items, pp.prep) => {
                // the object is a code span or URL that cannot take a case ending
                if let Some(pw) = case_prep(c) {
                    out.push(w(pw, pp.prep));
                }
            }
            (None, PrepRole::Case(c)) => {
                // stranded preposition / particle: keep an adverbial reading if we have one
                if let Some(c) = self.cand(prep, Cat::Adv) {
                    out.push(w(clean_form(c.entry.form), pp.prep));
                } else if prep.role().is_none() {
                    if let Some(c) = self.cand(prep, Cat::Prep) {
                        out.push(w(clean_form(c.entry.form), pp.prep));
                    }
                }
                if out.is_empty() {
                    if let Some(pw) = case_prep(c) {
                        out.push(w(pw, pp.prep));
                    }
                }
            }
        }
        if out.is_empty() && prep.role().is_none() {
            // foreign preposition without English role: use its Quenya candidate
            if let Some(c) = self.best_cand(prep) {
                out.push(w(clean_form(c.entry.form), pp.prep));
            }
            if let Some(np) = &pp.np {
                out.extend(self.render_np_ex(items, np, Case::Nom, false));
            }
        }
        out
    }

    fn render_wh(&self, items: &[Item], k: usize, clause_initial: bool, question: bool) -> Word {
        let it = &items[k];
        let key = it.role_key();
        let wd = match it.role() {
            Some(Role::Wh(wd)) => wd,
            Some(Role::Demonstrative { .. }) => "i",
            _ => {
                return match self.best_cand(it) {
                    Some(c) => w(clean_form(c.entry.form), k),
                    None => w(it.text.clone(), k),
                }
            }
        };
        if clause_initial && question {
            return w(wd, k);
        }
        match key {
            "who" | "whom" | "whoever" | "which" | "whichever" | "that" | "whose" => w(if key == "whose" { "yo" } else { "i" }, k),
            _ => w(wd, k),
        }
    }

    fn render_conj(&self, items: &[Item], k: usize) -> Word {
        let it = &items[k];
        match it.role() {
            Some(Role::Conj(c)) => w(c, k),
            // "that" introducing a clause: sa (PE22), not the relative i
            Some(Role::Demonstrative { .. }) => w("sa", k),
            Some(Role::Prep(PrepRole::Word(p))) => w(p, k),
            Some(Role::Wh(wd)) => w(wd, k),
            Some(Role::Word(wd)) => w(wd, k),
            _ => match self.cand(it, Cat::Conj).or_else(|| self.best_cand(it)) {
                Some(c) => w(clean_form(c.entry.form), k),
                None => {
                    let mut u = self.unknown_word(it, Number::Sg, Case::Nom, None);
                    u.src = Some(k);
                    u
                }
            },
        }
    }

    fn render_intj(&self, items: &[Item], k: usize) -> Word {
        let it = &items[k];
        match it.role() {
            Some(Role::Word(wd)) => w(wd, k),
            Some(Role::Quant("ú")) => w("lá", k),
            Some(Role::Neg) => w("lá", k),
            _ => match self.cand(it, Cat::Intj).or_else(|| self.best_cand(it)) {
                Some(c) => w(clean_form(c.entry.form), k),
                None => {
                    let mut u = self.unknown_word(it, Number::Sg, Case::Nom, None);
                    u.src = Some(k);
                    u
                }
            },
        }
    }
}

// ------------------------------------------------------------------ clause semantics

struct Ctx {
    prev_plural: bool,
    verb_key: String,
    question: bool,
    clause_initial: bool,
}

impl<'d> Translator<'d> {
    fn modal_entry(&mut self, a: Aux) -> Entry {
        match a {
            Aux::Can => self.verb_by_en(&["can", "be able"], "pol", synthetic_verb("pol-", "can")),
            Aux::May => self.verb_by_en(&["may", "be allowed"], "lerta", synthetic_verb("lerta-", "may")),
            _ => synthetic_verb("mauya-", "must"),
        }
    }

    fn render_modal(&mut self, items: &[Item], a: Aux, k: usize, subj: &mut Subject, neg: bool, plural_noun_subj: bool) -> Vec<Word> {
        let it = &items[k];
        let low = it.text.to_lowercase();
        let past = it.tags & f::PAST != 0 || matches!(low.as_str(), "could" | "might" | "had" | "should" | "would" | "ought");
        let tense = if past { Tense::Past } else { Tense::Aorist };
        let e = self.modal_entry(a);
        let mut out = Vec::new();
        match a {
            Aux::Must => {
                let mut m = if past { "mauyanë".to_string() } else { "mauya".to_string() };
                if neg {
                    m = format!("ui {}", m);
                }
                out.push(w(m, k));
                if let Some(p) = subj.person {
                    out.push(free(p.dative()));
                } else if let Some(np) = subj.np.take() {
                    out.extend(self.render_np(items, &np, Case::Dat));
                    subj.words.clear();
                }
            }
            _ => {
                let person = subj.person.or_else(|| if subj.words.is_empty() { person_of(it.tags) } else { None });
                let spec = VerbSpec { tense, subject: person, plural_subject: plural_noun_subj, dual_subject: subj.dual && person.is_none(), object: None, negative: neg };
                out.push(w(quenya::conjugate(&e, &spec), k));
            }
        }
        out
    }

    /// Render a verb group. `pred_plural` is set for existential clauses.
    fn render_verb(&mut self, items: &[Item], vp: &VP, subj: &mut Subject, object: Option<Person>, imperative_ok: bool, pred_plural: Option<bool>, question: bool) -> Vec<Word> {
        let ws = self.render_verb_inner(items, vp, subj, object, imperative_ok, pred_plural, question);
        stamp(ws, vp.verb.or_else(|| vp.span.first().copied()))
    }

    fn render_verb_inner(&mut self, items: &[Item], vp: &VP, subj: &mut Subject, object: Option<Person>, imperative_ok: bool, pred_plural: Option<bool>, question: bool) -> Vec<Word> {
        let vtags = vp.verb.map(|v| items[v].tags).unwrap_or(0);
        // negation as a verb suffix (Japanese 読まない, Korean 안/않)
        let neg = !vp.neg.is_empty() || vtags & f::NEG != 0 || vp.cops.iter().any(|&c| items[c].tags & f::NEG != 0);
        let mut out: Vec<Word> = Vec::new();
        for &a in &vp.advs_pre {
            out.push(self.render_adv_item(&items[a]));
        }
        let (mut has_fut, mut has_perf, mut perf_past, mut do_tense, mut modal, mut hortative) = (false, false, false, None, None, false);
        for &(k, a) in &vp.auxs {
            let low = items[k].text.to_lowercase();
            match a {
                Aux::Perfect => {
                    has_perf = true;
                    perf_past = items[k].tags & f::PAST != 0 || low == "had";
                }
                Aux::Future | Aux::Cond => has_fut = true,
                Aux::Do => do_tense = Some(if items[k].tags & f::PAST != 0 || low == "did" { Tense::Past } else { Tense::Aorist }),
                Aux::Can | Aux::May | Aux::Must => modal = Some((a, k)),
                Aux::Let => hortative = true,
            }
        }
        let cop_tense = vp.cops.first().and_then(|&c| tense_of(items[c].tags));
        let cop_pl = vp.cops.iter().any(|&c| items[c].tags & f::PL != 0 && items[c].tags & f::P3 != 0);
        // languages whose "have + participle" is the ordinary narrative past
        let narrative_perfect = matches!(self.an.lang.as_str(), "fr" | "it" | "de" | "nl" | "ro" | "ca" | "oc" | "lb" | "af" | "rm" | "fur" | "sc" | "yi" | "fy" | "li");
        let aux_tense = if has_fut && has_perf {
            Some(Tense::FuturePerfect)
        } else if has_perf {
            Some(if perf_past { Tense::PastPerfect } else if narrative_perfect { Tense::Past } else { Tense::Perfect })
        } else if has_fut {
            Some(Tense::Future)
        } else {
            do_tense
        };
        let plural_noun_subj = subj.person.is_none() && (subj.plural || cop_pl);
        let cop_person = |items: &[Item], subj: &Subject| -> Option<Person> {
            subj.person.or_else(|| if subj.words.is_empty() && !subj.plural { vp.cops.first().and_then(|&c| person_of(items[c].tags)) } else { None })
        };

        // existential: there is / there are / there came
        if let Some(pl) = pred_plural {
            let t = aux_tense.or(cop_tense).unwrap_or(Tense::Aorist);
            if let Some(v) = vp.verb {
                if let Some(c) = self.cand(&items[v], Cat::V) {
                    let t = aux_tense.or(tense_of(vtags)).unwrap_or(Tense::Aorist);
                    let spec = VerbSpec { tense: t, subject: None, plural_subject: pl, dual_subject: false, object: None, negative: neg };
                    out.push(free(quenya::conjugate(&c.entry, &spec)));
                    return out;
                }
            }
            if neg {
                out.push(free("lá"));
            }
            out.push(free(quenya::exist(t, pl)));
            return out;
        }

        // copular clause
        if vp.verb.is_none() && !vp.cops.is_empty() {
            let t = aux_tense.or(cop_tense).unwrap_or(Tense::Aorist);
            let person = cop_person(items, subj);
            if let Some((a, k)) = modal {
                out.extend(self.render_modal(items, a, k, subj, neg, plural_noun_subj));
                out.push(free("ná"));
            } else {
                out.push(free(quenya::copula(t, person, plural_noun_subj, neg)));
            }
            for &a in &vp.advs_post {
                out.push(self.render_adv_item(&items[a]));
            }
            return out;
        }

        // no lexical verb: auxiliaries used as full verbs ("I can", "I do"), or bare negation
        if vp.verb.is_none() {
            if let Some((a, k)) = modal {
                out.extend(self.render_modal(items, a, k, subj, neg, plural_noun_subj));
            } else if let Some(&(k, _)) = vp.auxs.first() {
                let it = &items[k];
                if let Some(c) = self.cand(it, Cat::V) {
                    let t = tense_of(it.tags).unwrap_or(Tense::Aorist);
                    let person = subj.person.or_else(|| if subj.words.is_empty() { person_of(it.tags) } else { None });
                    let spec = VerbSpec { tense: t, subject: person, plural_subject: plural_noun_subj, dual_subject: false, object, negative: neg };
                    out.push(w(quenya::conjugate(&c.entry, &spec), k));
                } else if neg {
                    out.push(free("lá"));
                }
            } else if neg {
                out.push(w("lá", vp.neg[0]));
            }
            for &a in &vp.advs_post {
                out.push(self.render_adv_item(&items[a]));
            }
            return out;
        }

        let v = vp.verb.unwrap();
        let vit = &items[v];
        let Some(c) = self.cand(vit, Cat::V) else {
            // no verbal reading: fall back to nominal / raw
            if neg {
                out.push(free("lá"));
            }
            if let Some(cn) = self.cand(vit, Cat::N) {
                out.push(free(quenya::noun(&cn.entry, Number::Sg, Case::Nom, None)));
            } else if let Some(ca) = self.cand(vit, Cat::Adj) {
                out.push(free(quenya::adjective(&ca.entry, Number::Sg)));
            } else if let Some(Role::Word(wd)) = vit.role() {
                out.push(w(wd, v));
            } else {
                let mut u = self.unknown_word(vit, Number::Sg, Case::Nom, None);
                u.src = Some(v);
                out.push(u);
            }
            for &a in &vp.advs_post {
                out.push(self.render_adv_item(&items[a]));
            }
            return out;
        };
        let e = c.entry.clone();
        let low = vit.text.to_lowercase();

        // imperative
        let english_imp = imperative_ok && subj.person.is_none() && subj.words.is_empty() && vp.auxs.is_empty() && vp.cops.is_empty() && self.an.is_english() && vtags & (f::PAST | f::PERF | f::PTCP | f::P3 | f::IMPF | f::FUT | f::PL) == 0 && !question && !low.ends_with("ing") && !low.ends_with("ed");
        let no_subject = subj.person.is_none() && subj.words.is_empty();
        let tagged_imp = vtags & f::IMP != 0 && vp.auxs.is_empty() && vp.cops.is_empty() && no_subject;
        // a form that is imperative in one of its readings ("ama!", "canta!") in a subjectless exclamation
        let exclaim_imp = self.exclaim && imperative_ok && no_subject && !question && vp.auxs.is_empty() && vp.cops.is_empty()
            && vit.an.as_ref().map(|a| a.senses.get(c.sense).map(|s| s.tagsets.iter().any(|&t| t & f::IMP != 0)).unwrap_or(false)).unwrap_or(false);
        if english_imp || tagged_imp || exclaim_imp || hortative {
            let mut s = if neg { format!("áva {}", quenya::aorist(&e)) } else { quenya::imperative(&e) };
            if let Some(o) = object {
                s = format!("{}{}", quenya::strip_diaeresis(&s), if is_third(o) && matches!(o, Person::P3 | Person::P3Inan) { "t" } else { "s" });
                s = quenya::respell(&s);
            }
            out.push(w(s, v));
            for &a in &vp.advs_post {
                out.push(self.render_adv_item(&items[a]));
            }
            return out;
        }

        // passive / progressive with copula
        if !vp.cops.is_empty() {
            let ct = aux_tense.or(cop_tense).unwrap_or(Tense::Aorist);
            let ing = low.ends_with("ing") || (vtags & (f::PRES | f::IMPF) != 0 && vtags & f::PTCP != 0);
            // Slavic "be + l-participle" is the ordinary active past ("je dao", "dal jsem")
            let slavic_perfect = matches!(self.an.lang.as_str(), "sh" | "sl" | "cs" | "sk" | "bg" | "mk")
                && vtags & f::PAST != 0
                && vtags & (f::PASS | f::PTCP) == 0
                && !ing;
            if slavic_perfect {
                let person = cop_person(items, subj);
                let spec = VerbSpec { tense: Tense::Past, subject: person, plural_subject: plural_noun_subj, dual_subject: subj.dual && person.is_none(), object, negative: neg };
                out.push(w(quenya::conjugate(&e, &spec), v));
                for &a in &vp.advs_post {
                    out.push(self.render_adv_item(&items[a]));
                }
                return out;
            }
            let passive = !ing && (vtags & f::PASS != 0 || vtags & f::PAST != 0 || vtags & f::PTCP != 0 || low.ends_with("ed") || low.ends_with("en"));
            if passive {
                let person = cop_person(items, subj);
                out.push(free(quenya::copula(ct, person, plural_noun_subj, neg)));
                let mut pp = quenya::passive_participle(&e);
                if subj.plural {
                    pp = quenya::adjective_plural(&pp);
                }
                out.push(w(pp, v));
            } else {
                let t = match ct {
                    Tense::Past | Tense::PastImperfect => Tense::PastImperfect,
                    Tense::Future | Tense::FutureImperfect => Tense::FutureImperfect,
                    Tense::Perfect | Tense::PastPerfect | Tense::FuturePerfect => ct,
                    _ => Tense::Present,
                };
                let person = cop_person(items, subj);
                let spec = VerbSpec { tense: t, subject: person, plural_subject: plural_noun_subj, dual_subject: subj.dual && person.is_none(), object, negative: neg };
                out.push(w(quenya::conjugate(&e, &spec), v));
            }
            for &a in &vp.advs_post {
                out.push(self.render_adv_item(&items[a]));
            }
            return out;
        }

        // modal + infinitive
        if let Some((a, k)) = modal {
            out.extend(self.render_modal(items, a, k, subj, neg, plural_noun_subj));
            let mut inf = quenya::aorist(&e);
            if let Some(o) = object {
                inf = quenya::respell(&format!("{}{}", quenya::strip_diaeresis(&inf), if matches!(o, Person::P3 | Person::P3Inan) { "t" } else { "s" }));
            }
            out.push(w(inf, v));
            for &a in &vp.advs_post {
                out.push(self.render_adv_item(&items[a]));
            }
            return out;
        }

        // finite verb
        let t = aux_tense.or_else(|| tense_of(vtags)).unwrap_or(Tense::Aorist);
        let person = subj.person.or_else(|| if subj.words.is_empty() && !subj.plural { person_of(vtags) } else { None });
        let spec = VerbSpec { tense: t, subject: person, plural_subject: plural_noun_subj, dual_subject: subj.dual && person.is_none(), object, negative: neg };
        out.push(w(quenya::conjugate(&e, &spec), v));
        for &a in &vp.advs_post {
            out.push(self.render_adv_item(&items[a]));
        }
        out
    }

    /// Source-case readings of a bare post-verbal NP that Quenya renders differently:
    /// predicate instrumentals/essives after a copula are nominative; a genitive=dative form
    /// (Romanian, German feminine) after a verb of giving is the recipient.
    fn bare_np_case(&self, items: &[Item], np: &NP, vp: &VP, ctx: &Ctx) -> Option<Case> {
        let h = np.head()?;
        let t = items[h].tags;
        if t & f::GEN != 0 && t & f::DAT != 0 {
            return Some(if vp.verb.is_some() && GIVING_VERBS.contains(&ctx.verb_key.as_str()) { Case::Dat } else { Case::Gen });
        }
        // Finnish/Hungarian allative recipient ("kuningattarelle")
        if t & f::ALL != 0 && t & f::NOM == 0 && vp.verb.is_some() && GIVING_VERBS.contains(&ctx.verb_key.as_str()) {
            return Some(Case::Dat);
        }
        if !vp.cops.is_empty() && vp.verb.is_none() && t & (f::INS | f::ESS | f::TRA) != 0 {
            return Some(Case::Nom);
        }
        None
    }

    /// A phrase whose case a particle decided (possibly ambiguously: に/에).
    fn render_forced_np(&mut self, items: &[Item], np: &NP, ctx: &mut Ctx) -> Vec<Word> {
        let h = np.head().or(np.pron);
        let forced = h.map(|h| items[h].forced).unwrap_or(0);
        let noun_key = h
            .and_then(|h| items[h].an.as_ref().and_then(|a| cand_for(a, Cat::N).and_then(|c| a.senses.get(c.sense).map(|s| s.key.clone()))))
            .unwrap_or_default();
        let text = h.map(|h| items[h].text.clone()).unwrap_or_default();
        let case = cjk::resolve_case(forced, &ctx.verb_key, &noun_key, &text).unwrap_or(Case::Nom);
        ctx.prev_plural = np.plural;
        ctx.clause_initial = false;
        self.render_np_ex(items, np, case, false)
    }

    fn render_node(&mut self, items: &[Item], n: &Node, ctx: &mut Ctx) -> Vec<Word> {
        let out = match n {
            Node::NP(np) => {
                ctx.prev_plural = np.plural;
                self.render_np(items, np, Case::Nom)
            }
            Node::PP(pp) => {
                let vk = ctx.verb_key.clone();
                self.render_pp(items, pp, &vk)
            }
            Node::Adj(k) => vec![self.render_adj_item(&items[*k], if ctx.prev_plural { Number::Pl } else { Number::Sg })],
            Node::Adv(k) => vec![self.render_adv_item(&items[*k])],
            Node::Conj(k) => vec![self.render_conj(items, *k)],
            Node::Wh(k) => vec![self.render_wh(items, *k, ctx.clause_initial, ctx.question)],
            Node::Intj(k) => vec![self.render_intj(items, *k)],
            Node::Other(k) => {
                let it = &items[*k];
                if it.protected {
                    vec![w(it.text.clone(), *k)]
                } else if let Some(Role::Word(wd)) = it.role() {
                    vec![w(wd, *k)]
                } else if let Some(c) = self.best_cand(it) {
                    vec![w(clean_form(c.entry.form), *k)]
                } else {
                    let mut u = self.unknown_word(it, Number::Sg, Case::Nom, None);
                    u.src = Some(*k);
                    vec![u]
                }
            }
            Node::VP(vp) => self.render_vp_nonmain(items, vp, ctx),
        };
        ctx.clause_initial = false;
        out
    }

    fn render_vp_nonmain(&mut self, items: &[Item], vp: &VP, ctx: &mut Ctx) -> Vec<Word> {
        let mut out = Vec::new();
        if vp.to.is_some() {
            for &a in &vp.advs_pre {
                out.push(self.render_adv_item(&items[a]));
            }
            if !vp.neg.is_empty() {
                out.push(free("lá"));
            }
            if let Some(v) = vp.verb {
                match self.cand(&items[v], Cat::V) {
                    Some(c) => out.push(w(quenya::aorist(&c.entry), v)),
                    None => out.push(self.render_noun_item(&items[v], Number::Sg, Case::Nom, None)),
                }
            } else if !vp.cops.is_empty() {
                out.push(free("ná"));
            }
            for &a in &vp.advs_post {
                out.push(self.render_adv_item(&items[a]));
            }
            return out;
        }
        if let Some(v) = vp.verb {
            let it = &items[v];
            let low = it.text.to_lowercase();
            if vp.auxs.is_empty() && vp.cops.is_empty() && (it.tags & f::PTCP != 0 || (self.an.is_english() && (low.ends_with("ing") || low.ends_with("ed")) && it.tags & (f::PAST | f::P3) == 0)) {
                if let Some(c) = self.cand(it, Cat::V) {
                    for &a in &vp.advs_pre {
                        out.push(self.render_adv_item(&items[a]));
                    }
                    if !vp.neg.is_empty() {
                        out.push(free("lá"));
                    }
                    let p = if it.tags & f::PRES != 0 || low.ends_with("ing") { quenya::active_participle(&c.entry) } else { quenya::passive_participle(&c.entry) };
                    out.push(w(if ctx.prev_plural { quenya::adjective_plural(&p) } else { p }, v));
                    for &a in &vp.advs_post {
                        out.push(self.render_adv_item(&items[a]));
                    }
                    return out;
                }
            }
        }
        let mut s = Subject { person: None, plural: ctx.prev_plural, dual: false, words: Vec::new(), np: None };
        self.render_verb(items, vp, &mut s, None, false, None, ctx.question)
    }

    fn render_clause(&mut self, items: &mut Vec<Item>, idxs_all: &[usize], terminal: Option<char>, question: bool, sentence_first: bool, continuation: bool) {
        let idxs: Vec<usize> = idxs_all.iter().copied().filter(|&k| !items[k].protected).collect();
        if idxs.is_empty() {
            return;
        }
        self.exclaim = terminal == Some('!');
        // determiner ↔ noun agreement narrows ambiguous case readings (German "der Königin")
        if !self.an.is_english() {
            let mut pending: Option<u64> = None;
            for &k in &idxs {
                match items[k].tag {
                    Tag::Det | Tag::Poss => {
                        let c = det_constraints(&items[k]);
                        pending = if c != 0 { Some(c) } else { pending };
                    }
                    Tag::Adj | Tag::Num => {}
                    Tag::N => {
                        if let Some(c) = pending.take() {
                            refine_noun_tags(items, k, c);
                        }
                    }
                    _ => pending = None,
                }
            }
        }
        let caps_nouns = matches!(self.lang(), "de" | "lb");
        let heading = idxs.len() >= 2
            && idxs.iter().filter(|&&k| items[k].cap != Cap::Lower).count() >= 2
            && idxs.iter().all(|&k| items[k].cap != Cap::Lower || (!caps_nouns && items[k].text.chars().count() <= 3) || items[k].kind == Kind::Number);
        let all_upper = idxs.iter().filter(|&&k| items[k].kind == Kind::Word).all(|&k| items[k].cap == Cap::Upper) && idxs.iter().any(|&k| items[k].text.chars().filter(|c| c.is_alphabetic()).count() >= 2);
        let mut nodes = parse(items, &idxs, self.an.is_english());
        // 何を / 誰が / 什么名字 / 在哪里: a question word inside a noun or prepositional phrase
        let np_wh = |np: &NP| np.span.iter().any(|&k| wh_word(&items[k]).is_some());
        let wh_node = |n: &Node| match n {
            Node::Wh(_) => true,
            Node::NP(np) => np_wh(np),
            Node::PP(pp) => pp.np.as_ref().map(np_wh).unwrap_or(false),
            _ => false,
        };
        let in_situ = matches!(self.lang(), "zh" | "yue" | "lzh" | "nan" | "hak" | "ja" | "ko");
        if in_situ {
            // 你在哪里: an adverbial question word absorbed by the verb group stands alone
            let mut pulled: Option<usize> = None;
            for n in nodes.iter_mut() {
                if let Node::VP(vp) = n {
                    for list in [&mut vp.advs_pre, &mut vp.advs_post] {
                        if let Some(p) = list.iter().position(|&k| wh_word(&items[k]).is_some()) {
                            let k = list.remove(p);
                            vp.span.retain(|&x| x != k);
                            pulled = Some(k);
                        }
                    }
                }
                if pulled.is_some() {
                    break;
                }
            }
            if let Some(k) = pulled {
                nodes.insert(0, Node::Wh(k));
            }
        }
        let has_wh = nodes.iter().any(wh_node);
        let mut wh_fronted = false;
        if has_wh && in_situ {
            // Quenya fronts the question word: 你为什么哭 → manan nítal
            if let Some(wi) = nodes.iter().position(wh_node) {
                if wi > 0 {
                    // 什么名字: "what" takes its noun phrase along
                    let takes_np = matches!(&nodes[wi], Node::Wh(k) if matches!(wh_word(&items[*k]), Some("mana" | "ya")))
                        && matches!(nodes.get(wi + 1), Some(Node::NP(np)) if np_forced(items, np) == 0 && np.pron.is_none());
                    let wh = nodes.remove(wi);
                    if takes_np {
                        let np = nodes.remove(wi);
                        nodes.insert(0, np);
                    }
                    // 住在哪里: the preposition whose object the question word was goes with it
                    if wi >= 1 && matches!(&nodes[wi - 1], Node::PP(pp) if pp.np.is_none()) && matches!(&wh, Node::Wh(k) if wh_word(&items[*k]) == Some("massë")) {
                        nodes.remove(wi - 1);
                    }
                    nodes.insert(0, wh);
                }
                wh_fronted = true;
            }
        }
        let mut ctx = Ctx { prev_plural: false, verb_key: String::new(), question, clause_initial: true };
        let mut words: Vec<Word> = Vec::new();

        let main = nodes.iter().position(|n| matches!(n, Node::VP(vp) if vp.to.is_none() && vp.has_head()));
        if std::env::var_os("TOLKIEN_DEBUG").is_some() {
            let kinds: Vec<String> = nodes
                .iter()
                .map(|n| match n {
                    Node::NP(np) => format!("NP{:?}", np.span.iter().map(|&k| items[k].text.as_str()).collect::<Vec<_>>()),
                    Node::VP(vp) => format!("VP{:?}", vp.span.iter().map(|&k| items[k].text.as_str()).collect::<Vec<_>>()),
                    Node::PP(pp) => format!("PP[{}]", items[pp.prep].text),
                    Node::Adj(k) => format!("Adj[{}]", items[*k].text),
                    Node::Adv(k) => format!("Adv[{}]", items[*k].text),
                    Node::Conj(k) => format!("Conj[{}]", items[*k].text),
                    Node::Wh(k) => format!("Wh[{}]", items[*k].text),
                    Node::Intj(k) => format!("Intj[{}]", items[*k].text),
                    Node::Other(k) => format!("Other[{}]", items[*k].text),
                })
                .collect();
            let tags: Vec<String> = idxs_all.iter().map(|&k| format!("{}/{}", items[k].text, items[k].tag.name())).collect();
            eprintln!("tags {}", tags.join(" "));
            eprintln!("nodes {} main={:?}", kinds.join(" "), main);
        }
        let verb_final = matches!(self.lang(), "ja" | "ko");
        let Some(m) = main else {
            let snapshot = nodes.clone();
            // "王は賢い": topic + predicate without an overt copula
            let sinitic = matches!(self.lang(), "zh" | "yue" | "lzh" | "nan" | "hak");
            let topic = if snapshot.len() >= 2 {
                matches!(&snapshot[0], Node::NP(np) if (verb_final && np_forced(items, np) & f::NOM != 0) || (sinitic && np_forced(items, np) == 0 && !np.there))
            } else {
                false
            };
            // "剑很长": the rest is an adjectival predicate (with adverbs), or a nominal one in Japanese/Korean
            let adjectival = |n: &Node| matches!(n, Node::Adj(_)) || matches!(n, Node::NP(np) if np.head().is_none() && np.pron.is_none() && !np.adjs.is_empty());
            let predicate = topic
                && snapshot[1..].iter().any(|n| adjectival(n) || (verb_final && matches!(n, Node::NP(_))) || (sinitic && matches!(n, Node::PP(_))))
                && snapshot[1..].iter().all(|n| matches!(n, Node::Adj(_) | Node::Adv(_) | Node::NP(_) | Node::Other(_) | Node::Conj(_) | Node::PP(_)));
            let topic_plural = matches!(&snapshot[0], Node::NP(np) if np.plural);
            for (i, n) in snapshot.iter().enumerate() {
                let ws = match n {
                    // "딸은 아름답다": a bare adjective after the topic is a predicate, not a substantive
                    Node::NP(np) if i > 0 && predicate && np.head().is_none() && np.pron.is_none() && !np.adjs.is_empty() => {
                        let num = if topic_plural { Number::Pl } else { Number::Sg };
                        np.adjs.iter().map(|&k| self.render_adj_item(&items[k], num)).collect()
                    }
                    Node::NP(np) if np_forced(items, np) != 0 && (i > 0 || !topic) => self.render_forced_np(items, np, &mut ctx),
                    _ => self.render_node(items, n, &mut ctx),
                };
                words.extend(ws);
                if i == 0 && predicate {
                    let plural = matches!(&snapshot[0], Node::NP(np) if np.plural);
                    words.push(free(if plural { "nar" } else { "ná" }));
                }
            }
            self.finish_clause(items, idxs_all, &idxs, words, question && sentence_first && !continuation && !has_wh, heading, all_upper);
            return;
        };

        // negation outside the verb group (German "nicht" at the end) belongs to the main verb
        let mut stray_neg: Vec<usize> = Vec::new();
        let mut i = 0;
        while i < nodes.len() {
            let take = match &nodes[i] {
                Node::Other(k) | Node::Adv(k) if items[*k].tag == Tag::Neg || matches!(items[*k].role(), Some(Role::Neg)) => Some(vec![*k]),
                Node::VP(v) if i != m && v.verb.is_none() && v.cops.is_empty() && v.auxs.is_empty() && !v.neg.is_empty() && v.advs_pre.is_empty() && v.advs_post.is_empty() => Some(v.neg.clone()),
                _ => None,
            };
            if let Some(ks) = take {
                stray_neg.extend(ks);
                nodes.remove(i);
            } else {
                i += 1;
            }
        }
        let m = nodes.iter().position(|n| matches!(n, Node::VP(vp) if vp.to.is_none() && vp.has_head())).unwrap_or(m.min(nodes.len().saturating_sub(1)));
        if !stray_neg.is_empty() {
            if let Node::VP(v) = &mut nodes[m] {
                v.neg.extend(stray_neg);
            }
        }

        // subject and inversion
        let mut subj_node: Option<usize> = None;
        let mut inverted = false;
        let aux_only = matches!(&nodes[m], Node::VP(vp) if vp.verb.is_none() && (!vp.auxs.is_empty() || !vp.cops.is_empty()));
        // Romance/Slavic object clitics before the verb ("je le vois", "ti amo") are never the subject
        let clitics = !self.an.is_english();
        let is_obj_pron = |np: &NP| np.pron.map(|p| matches!(items[p].role(), Some(Role::Pronoun { obj: true, refl: false, .. }))).unwrap_or(false);
        let is_plain_pron = |np: &NP| np.pron.map(|p| matches!(items[p].role(), Some(Role::Pronoun { refl: false, .. }))).unwrap_or(false);
        // all person/number readings of the verb: "je vous aime" → the pronoun agreeing with
        // the verb is the subject, the other one its object
        let vtags_all = match &nodes[m] {
            Node::VP(vp) => vp
                .verb
                .and_then(|v| items[v].an.as_ref().and_then(|a| cand_for(a, Cat::V).and_then(|c| a.senses.get(c.sense)).map(|s| s.tagsets.iter().filter(|t| *t & f::IMP == 0).fold(0u64, |acc, t| acc | t))))
                .unwrap_or(0),
            _ => 0,
        };
        let agrees = |np: &NP| -> bool {
            let Some(p) = np.person(items) else { return false };
            let pb = match p {
                Person::S1 | Person::P1Ex | Person::P1In | Person::D1Ex | Person::D1In => f::P1,
                Person::S2Fam | Person::S2Pol | Person::P2 | Person::D2 => f::P2,
                _ => f::P3,
            };
            if vtags_all & pb == 0 {
                return false;
            }
            let pl = p.is_plural() || p.is_dual();
            if vtags_all & f::NUMBER != 0 && ((pl && vtags_all & f::PL == 0) || (!pl && vtags_all & f::SG == 0)) {
                return false;
            }
            true
        };
        let post_nom_pron = matches!(nodes.get(m + 1), Some(Node::NP(np)) if np.pron.map(|p| matches!(items[p].role(), Some(Role::Pronoun { obj: false, .. }))).unwrap_or(false));
        // "Книгу читает мальчик": a bare noun after the verb that can be nominative and agrees
        // with it in number is a possible subject, which makes a case-marked noun before the
        // verb the object
        let post_nom_noun = clitics
            && matches!(nodes.get(m + 1), Some(Node::NP(np)) if np.pron.is_none() && !np.there && np_forced(items, np) == 0 && np.head().map(|h| {
                let c = noun_cases(&items[h]);
                let pl = items[h].tags & f::PL != 0;
                (c == 0 || c & f::NOM != 0) && !(vtags_all & f::NUMBER != 0 && ((pl && vtags_all & f::PL == 0) || (!pl && vtags_all & f::SG == 0)))
            }).unwrap_or(false));
        let other_subj = post_nom_pron || post_nom_noun;
        let np_before = nodes[..m]
            .iter()
            .rposition(|n| matches!(n, Node::NP(np) if np_forced(items, np) & f::NOM != 0))
            .or_else(|| {
                if clitics && vtags_all & f::PERSONS != 0 {
                    let prons = nodes[..m].iter().filter(|n| matches!(n, Node::NP(np) if np_forced(items, np) == 0 && is_plain_pron(np))).count();
                    if prons >= 2 {
                        return nodes[..m].iter().rposition(|n| matches!(n, Node::NP(np) if np_forced(items, np) == 0 && is_plain_pron(np) && !is_obj_pron(np) && agrees(np)));
                    }
                }
                None
            })
            // "dass er den König gesehen hat": an accusative/dative-marked phrase is the object
            .or_else(|| {
                if clitics {
                    nodes[..m].iter().rposition(|n| matches!(n, Node::NP(np) if np_forced(items, np) == 0 && !is_obj_pron(np) && !np_oblique_only(items, np, vtags_all, other_subj)))
                } else {
                    None
                }
            })
            .or_else(|| nodes[..m].iter().rposition(|n| matches!(n, Node::NP(np) if np_forced(items, np) == 0 && !(clitics && is_obj_pron(np)) && !(clitics && np_oblique_only(items, np, vtags_all, other_subj)))))
            .or_else(|| {
                // only case-marked objects precede the verb ("den König sah ich", "króla
                // widziałem"): the subject follows it or sits in the verb ending
                if clitics && (other_subj || vtags_all & f::PERSONS != 0) {
                    None
                } else {
                    nodes[..m].iter().rposition(|n| matches!(n, Node::NP(np) if np_forced(items, np) == 0 && !(clitics && is_obj_pron(np))))
                }
            });
        if np_before.is_none() && aux_only && matches!(nodes.get(m + 1), Some(Node::NP(_))) {
            if let Some(li) = (m + 2..nodes.len()).find(|&i| matches!(&nodes[i], Node::VP(v) if v.to.is_none())) {
                let between_ok = (m + 2..li).all(|i| matches!(nodes[i], Node::Adv(_)));
                if between_ok {
                    if let Node::VP(lv) = nodes[li].clone() {
                        if let Node::VP(mv) = &mut nodes[m] {
                            mv.auxs.extend(lv.auxs);
                            mv.cops.extend(lv.cops);
                            mv.neg.extend(lv.neg);
                            mv.advs_pre.extend(lv.advs_pre);
                            if mv.verb.is_none() {
                                mv.verb = lv.verb;
                            }
                            mv.advs_post.extend(lv.advs_post);
                        }
                        nodes.remove(li);
                    }
                }
            }
            subj_node = Some(m + 1);
            inverted = true;
        } else if np_before.is_none() && !self.an.is_english() {
            // verb-initial order (questions, Romance/Germanic inversion): a nominative
            // pronoun right after the verb is the subject
            if let Some(Node::NP(np)) = nodes.get(m + 1) {
                let nominative_pron = np.pron.map(|p| matches!(items[p].role(), Some(Role::Pronoun { obj: false, .. }))).unwrap_or(false);
                let fronted_object = post_nom_noun && nodes[..m].iter().any(|n| matches!(n, Node::NP(o) if np_forced(items, o) == 0 && np_oblique_only(items, o, vtags_all, true)));
                if nominative_pron || fronted_object {
                    subj_node = Some(m + 1);
                    inverted = true;
                }
            }
        } else {
            subj_node = np_before;
        }
        if wh_fronted && subj_node.is_some() {
            // 这是什么 → mana ná sina: the fronted question word is followed by the verb
            if let Node::VP(v) = &nodes[m] {
                if v.verb.is_none() && !v.cops.is_empty() {
                    inverted = true;
                }
            }
        }

        let vp = match &nodes[m] {
            Node::VP(v) => v.clone(),
            _ => unreachable!(),
        };
        ctx.verb_key = vp
            .verb
            .and_then(|v| {
                items[v].an.as_ref().and_then(|a| {
                    cand_for(a, Cat::V)
                        .and_then(|c| a.senses.get(c.sense).map(|s| s.key.clone()))
                        .or_else(|| a.senses.first().map(|s| s.key.clone()))
                })
            })
            .unwrap_or_default();

        let mut subj = Subject { person: None, plural: false, dual: false, words: Vec::new(), np: None };
        let mut existential: Option<bool> = None;
        // 猫がいる / 本がある: a bare subject with いる/ある and no complement is "there is a cat"
        let mut post_subject: Vec<Word> = Vec::new();
        let mut vp = vp;
        if verb_final {
            let head = vp.verb.or(vp.cops.first().copied());
            let exist_verb = head.map(|k| cjk::is_existential(self.lang(), &items[k].text)).unwrap_or(false);
            let neg_exist = head.map(|k| items[k].text.starts_with("없") || matches!(items[k].text.as_str(), "ない" | "いない" | "なかった" | "いなかった")).unwrap_or(false);
            let bare = nodes.iter().enumerate().all(|(i, n)| i == m || Some(i) == subj_node || matches!(n, Node::Adv(_) | Node::Intj(_) | Node::Conj(_) | Node::Other(_)));
            if exist_verb && bare {
                if let Some(Node::NP(np)) = subj_node.map(|si| &nodes[si]) {
                    if np.person(items).is_none() && !np.there {
                        existential = Some(np.plural);
                        post_subject = self.render_np(items, np, Case::Nom);
                        if let Some(v) = vp.verb.take() {
                            vp.cops.insert(0, v);
                        }
                        if neg_exist {
                            if let Some(&c) = vp.cops.first() {
                                if !vp.neg.contains(&c) {
                                    vp.neg.push(c);
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(si) = subj_node.filter(|_| existential.is_none()) {
            if let Node::NP(np) = &nodes[si] {
                if np.there {
                    // plurality of the predicate NP decides agreement
                    let pred = nodes.iter().enumerate().skip(m + 1).find(|(i, n)| *i != si && matches!(n, Node::NP(_)));
                    let pl = match pred {
                        Some((_, Node::NP(p))) => p.plural,
                        _ => false,
                    };
                    existential = Some(pl);
                } else if let Some(p) = np.person(items) {
                    let refl = matches!(items[np.pron.unwrap()].role(), Some(Role::Pronoun { refl: true, .. }));
                    if refl {
                        subj.words = self.render_np(items, np, Case::Nom);
                    } else {
                        subj.person = Some(p);
                        subj.plural = p.is_plural();
                        subj.dual = p.is_dual();
                    }
                } else {
                    subj.plural = np.plural;
                    subj.words = self.render_np(items, np, Case::Nom);
                    subj.np = Some(np.clone());
                    ctx.prev_plural = np.plural;
                }
            }
        }
        // coordinated subject ("ty i ja", "the king and I", "the king and the queen"): emphatic
        // pronouns joined by "ar", verb agreeing with the combined person (elyë ar inyë cenilvë)
        let mut coord_skip: Vec<usize> = Vec::new();
        if let Some(si) = subj_node.filter(|_| existential.is_none()) {
            if si >= 2 && matches!(nodes[si - 1], Node::Conj(_)) && items[match &nodes[si - 1] { Node::Conj(k) => *k, _ => 0 }].role_key() == "and" {
                if let (Node::NP(a), Node::NP(b)) = (&nodes[si - 2], &nodes[si]) {
                    let (pa, pb) = (a.person(items), b.person(items));
                    if !a.there && !b.there {
                        let rank = |p: Option<Person>| match p {
                            Some(Person::S1 | Person::P1Ex | Person::P1In | Person::D1Ex | Person::D1In) => 1,
                            Some(Person::S2Fam | Person::S2Pol | Person::P2 | Person::D2) => 2,
                            _ => 3,
                        };
                        let (ra, rb) = (rank(pa), rank(pb));
                        let combined = match (ra.min(rb), ra.max(rb)) {
                            (1, 2) => Person::P1In,
                            (1, _) => Person::P1Ex,
                            (2, _) => Person::P2,
                            _ => Person::P3,
                        };
                        let mut ws: Vec<Word> = Vec::new();
                        for (np, p) in [(a, pa), (b, pb)] {
                            match p {
                                Some(p) => ws.push(free(p.emphatic())),
                                None => ws.extend(self.render_np(items, np, Case::Nom)),
                            }
                            if ws.len() > 0 && std::ptr::eq(np, a) {
                                ws.push(free("ar"));
                            }
                        }
                        subj.words = ws;
                        subj.person = if pa.is_none() && pb.is_none() { None } else { Some(combined) };
                        subj.plural = true;
                        subj.dual = false;
                        subj.np = None;
                        coord_skip.push(si - 2);
                        coord_skip.push(si - 1);
                    }
                }
            }
        }
        // coordinated subjects ("the king and the queen are"): plural agreement from the verb form
        if subj.person.is_none() {
            let vpl = vp.cops.iter().chain(vp.verb.iter()).any(|&k| items[k].tags & f::PL != 0 && items[k].tags & f::P3 != 0);
            if vpl && subj_node.map(|si| si >= 2 && matches!(nodes[si - 1], Node::Conj(_))).unwrap_or(false) {
                subj.plural = true;
            }
        }

        // object suffix: 3rd person pronoun object directly after the verb, pronominal
        // (or pro-dropped: "widzę go" → ceninyes) subject
        let mut object: Option<Person> = None;
        let mut object_node: Option<usize> = None;
        let pro_drop = subj_node.is_none() && subj.words.is_empty() && !self.an.is_english() && vp.verb.map(|v| person_of(items[v].tags).is_some()).unwrap_or(false);
        if (subj.person.is_some() || pro_drop) && existential.is_none() {
            let first_after = (m + 1..nodes.len()).find(|&i| Some(i) != subj_node);
            // a pre-verbal object clitic ("lo veo", "je le vois", "il nous voit") counts as well
            let clitic = if clitics {
                (0..m).rev().find(|&i| Some(i) != subj_node && !matches!(nodes.get(i + 1), Some(Node::Conj(_))) && matches!(&nodes[i], Node::NP(np) if np_forced(items, np) == 0 && (is_obj_pron(np) || (subj_node.is_some() && is_plain_pron(np)))))
            } else {
                None
            };
            for oi in first_after.into_iter().chain(clitic) {
                if let Node::NP(np) = &nodes[oi] {
                    // "gave him a horse" / "je lui ai donné le livre": the pronoun is the
                    // indirect object (dative), not a suffix
                    let bare_np = |i: usize| matches!(nodes.get(i), Some(Node::NP(next)) if np_forced(items, next) == 0 && !next.there && next.pron.is_none());
                    let ditransitive = if oi > m { bare_np(oi + 1) } else { bare_np(m + 1) && GIVING_VERBS.contains(&ctx.verb_key.as_str()) };
                    let dative = np.pron.map(|p| items[p].tags & f::DAT != 0 && items[p].tags & (f::ACC | f::NOM) == 0).unwrap_or(false);
                    if let Some(p) = np.pron.filter(|_| !ditransitive && !dative) {
                        if let Some(Role::Pronoun { p: op, refl: false, .. }) = items[p].role() {
                            if is_third(op) {
                                object = Some(op);
                                object_node = Some(oi);
                                break;
                            }
                        }
                    }
                }
            }
        }

        // pre-verbal material
        let imperative_ok = subj_node.is_none() && !inverted && nodes[..m].iter().all(|n| matches!(n, Node::Intj(_) | Node::Adv(_) | Node::Conj(_)));
        let snapshot: Vec<Node> = nodes.clone();
        // particle-marked arguments (and every pre-verbal phrase of a verb-final language)
        // follow the verb in Quenya
        let mut deferred: Vec<usize> = Vec::new();
        for (i, n) in snapshot.iter().enumerate().take(m) {
            if Some(i) == subj_node || Some(i) == object_node || coord_skip.contains(&i) {
                continue;
            }
            if let Node::NP(np) = n {
                let fc = np_forced(items, np);
                // object clitics follow the verb as independent pronouns ("ti amo" → melin tye)
                if clitics && fc == 0 && !matches!(snapshot.get(i + 1), Some(Node::Conj(_))) && (is_obj_pron(np) || (subj_node.is_some() && is_plain_pron(np))) {
                    deferred.push(i);
                    continue;
                }
                // "王と女王が": the first conjunct of a coordinated subject stays with it
                let coordinated = matches!(snapshot.get(i + 1), Some(Node::Conj(_))) && subj_node == Some(i + 2);
                // "den König sah ich": a case-marked object fronted or before a clause-final verb
                let case_object = clitics && (subj_node.is_some() || pro_drop) && np_oblique_only(items, np, vtags_all, subj_node.is_some());
                if !coordinated && !(wh_fronted && i == 0) && ((fc != 0 && fc & f::NOM == 0) || (verb_final && !np.there) || case_object) {
                    deferred.push(i);
                    continue;
                }
            }
            // pre-verbal prepositional phrases of verb-final/Chinese clauses follow the verb too
            if matches!(n, Node::PP(_)) && (verb_final || matches!(self.lang(), "zh" | "yue" | "lzh" | "nan" | "hak")) && subj_node.map(|si| i > si).unwrap_or(true) {
                deferred.push(i);
                continue;
            }
            let ws = self.render_node(items, n, &mut ctx);
            words.extend(ws);
        }
        ctx.clause_initial = false;
        // no verb agreement in the source (Japanese/Korean/Chinese): a coordinated subject is plural
        if subj.person.is_none() && !subj.plural && (verb_final || matches!(self.lang(), "zh" | "yue" | "lzh" | "nan" | "hak" | "vi" | "th" | "id" | "ms")) {
            if subj_node.map(|si| si >= 2 && matches!(snapshot[si - 1], Node::Conj(_)) && matches!(snapshot[si - 2], Node::NP(_))).unwrap_or(false) {
                subj.plural = true;
            }
        }

        let vwords = self.render_verb(items, &vp, &mut subj, object, imperative_ok, existential, question);
        if inverted && has_wh {
            words.extend(vwords);
            words.extend(std::mem::take(&mut subj.words));
        } else {
            words.extend(std::mem::take(&mut subj.words));
            words.extend(vwords);
        }
        words.extend(std::mem::take(&mut post_subject));

        // post-verbal material
        let giving = vp.verb.is_some() && GIVING_VERBS.contains(&ctx.verb_key.as_str());
        let mut bare_nps: Vec<usize> = Vec::new();
        let mut forced_acc = false;
        for (i, n) in snapshot.iter().enumerate().skip(m + 1) {
            if Some(i) == subj_node || Some(i) == object_node {
                continue;
            }
            match n {
                Node::NP(np) if np_forced(items, np) != 0 => forced_acc |= np_forced(items, np) & f::ACC != 0,
                Node::NP(_) => bare_nps.push(i),
                // a conjunction or relative word ends the argument list ("the song that my mother sang")
                Node::PP(_) | Node::VP(_) | Node::Conj(_) | Node::Wh(_) => break,
                _ => {}
            }
        }
        let mut deferred_bare: Vec<usize> = Vec::new();
        for &i in &deferred {
            if let Node::NP(np) = &snapshot[i] {
                let fc = np_forced(items, np);
                if fc == 0 {
                    deferred_bare.push(i);
                } else {
                    forced_acc |= fc & f::ACC != 0;
                }
            }
        }
        let all_bare: Vec<usize> = deferred_bare.iter().chain(bare_nps.iter()).copied().collect();
        let indirect = if all_bare.len() >= 2 && vp.cops.is_empty() {
            Some(all_bare[0])
        } else if all_bare.len() == 1 && forced_acc && giving {
            Some(all_bare[0])
        } else {
            None
        };
        let mut deferred_words: Vec<Word> = Vec::new();
        for &i in &deferred {
            let ws = match &snapshot[i] {
                Node::NP(np) if Some(i) == indirect => {
                    ctx.prev_plural = np.plural;
                    self.render_np(items, np, Case::Dat)
                }
                Node::NP(np) if np_forced(items, np) != 0 => self.render_forced_np(items, np, &mut ctx),
                n => self.render_node(items, n, &mut ctx),
            };
            deferred_words.extend(ws);
        }
        // "飲みたい" = want + to drink: the arguments follow the whole verb chain
        let chain_end = (m + 1..snapshot.len()).take_while(|&i| matches!(&snapshot[i], Node::VP(v) if v.to.is_some())).last();
        if chain_end.is_none() {
            words.extend(std::mem::take(&mut deferred_words));
        }
        let subj_plural = subj.plural;
        for (i, n) in snapshot.iter().enumerate().skip(m + 1) {
            if Some(i) == subj_node || Some(i) == object_node {
                continue;
            }
            let ws = match n {
                Node::NP(np) if Some(i) == indirect => self.render_np(items, np, Case::Dat),
                Node::NP(np) if np_forced(items, np) != 0 => self.render_forced_np(items, np, &mut ctx),
                Node::VP(_) if chain_end == Some(i) => {
                    let mut ws = self.render_node(items, n, &mut ctx);
                    ws.extend(std::mem::take(&mut deferred_words));
                    ws
                }
                Node::NP(np) if indirect.is_none() && giving && np.dative_dependent(items).is_some() => {
                    let d = np.dative_dependent(items).unwrap();
                    let first = np.gens.contains(&d);
                    let mut rest = np.clone();
                    rest.postgens.retain(|&g| g != d);
                    rest.gens.retain(|&g| g != d);
                    ctx.prev_plural = rest.plural;
                    let mut dnp = NP::default();
                    dnp.nouns.push(d);
                    dnp.span.push(d);
                    dnp.plural = items[d].tags & f::PL != 0;
                    dnp.def = items[d].tags & f::DEF != 0;
                    let dw = self.render_np(items, &dnp, Case::Dat);
                    let mut ws = self.render_np(items, &rest, Case::Nom);
                    if first {
                        ws.splice(0..0, dw);
                    } else {
                        ws.extend(dw);
                    }
                    ws
                }
                Node::NP(np) if self.bare_np_case(items, np, &vp, &ctx).is_some() => {
                    let c = self.bare_np_case(items, np, &vp, &ctx).unwrap();
                    ctx.prev_plural = np.plural;
                    self.render_np_ex(items, np, c, false)
                }
                Node::Adj(k) if !vp.cops.is_empty() || existential.is_some() => vec![self.render_adj_item(&items[*k], if subj_plural { Number::Pl } else { Number::Sg })],
                _ => self.render_node(items, n, &mut ctx),
            };
            words.extend(ws);
        }
        let ma = question && sentence_first && !continuation && !has_wh;
        self.finish_clause(items, idxs_all, &idxs, words, ma, heading, all_upper);
    }

    fn finish_clause(&mut self, items: &mut Vec<Item>, idxs_all: &[usize], idxs: &[usize], mut words: Vec<Word>, question_particle: bool, heading: bool, all_upper: bool) {
        if question_particle {
            let replaced = if let Some(first) = words.first_mut() {
                if let Some(s) = first.src {
                    if items[s].tag == Tag::Conj && matches!(items[s].role_key(), "whether" | "if") {
                        first.text = "ma".to_string();
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };
            if !replaced {
                words.insert(0, free("ma"));
            }
        }
        let mut texts: Vec<(String, Option<usize>)> = words
            .iter()
            .map(|wd| {
                let mut t = wd.text.clone();
                if t.is_empty() || wd.keep {
                    return (t, wd.src);
                }
                if let Some(s) = wd.src {
                    let it = &items[s];
                    let letters = it.text.chars().filter(|c| c.is_alphabetic()).count();
                    if it.cap == Cap::Upper && letters >= 2 {
                        t = t.to_uppercase();
                    } else if it.cap == Cap::Title && (wd.name || heading) {
                        t = title_case(&t);
                    }
                } else if heading || wd.name {
                    t = title_case(&t);
                }
                if all_upper {
                    t = t.to_uppercase();
                }
                (t, wd.src)
            })
            .filter(|t| !t.0.is_empty())
            .collect();
        if std::env::var_os("TOLKIEN_DEBUG").is_some() {
            let ws: Vec<String> = texts.iter().map(|(t, s)| match s { Some(k) => format!("{}←{}", t, items[*k].text), None => t.clone() }).collect();
            eprintln!("words {}", ws.join(" | "));
        }
        if idxs_all.len() > idxs.len() {
            // protected tokens (code, URLs) go right after the last rendered word that came
            // from a source word preceding them, so "the `foo` function" keeps its shape;
            // sufmin[p] = smallest source index among texts[p..], non-decreasing, so the last
            // text from a source word before k is found by binary search (a line may hold
            // hundreds of thousands of protected tokens: hex dumps, byte tables)
            let nt = texts.len();
            let mut sufmin = vec![usize::MAX; nt + 1];
            for p in (0..nt).rev() {
                sufmin[p] = sufmin[p + 1].min(texts[p].1.unwrap_or(usize::MAX));
            }
            let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); nt + 1];
            let mut prev_bucket: Option<usize> = None;
            for (pos, &k) in idxs_all.iter().enumerate() {
                if !items[k].protected {
                    continue;
                }
                let last = sufmin[..nt].partition_point(|&m| m < k);
                let mut b = if last > 0 {
                    last
                } else {
                    // only free words precede it: keep its slot position
                    prev_bucket.unwrap_or(pos.min(nt))
                };
                if let Some(pb) = prev_bucket {
                    b = b.max(pb);
                }
                buckets[b].push(k);
                prev_bucket = Some(b);
            }
            let mut merged: Vec<(String, Option<usize>)> = Vec::with_capacity(nt + idxs_all.len() - idxs.len());
            for (b, t) in texts.into_iter().enumerate() {
                for &k in &buckets[b] {
                    merged.push((items[k].text.clone(), Some(k)));
                }
                merged.push(t);
            }
            for &k in &buckets[nt] {
                merged.push((items[k].text.clone(), Some(k)));
            }
            texts = merged;
            // each protected token keeps its own slot (quotes and brackets around it stay
            // in place); the words between two protected tokens fill the slots between them
            let is_prot = |t: &(String, Option<usize>)| t.1.map(|s| items[s].protected).unwrap_or(false);
            let mut seg_slots: Vec<Vec<usize>> = vec![Vec::new()];
            for &k in idxs_all {
                if items[k].protected {
                    seg_slots.push(Vec::new());
                } else {
                    seg_slots.last_mut().unwrap().push(k);
                }
            }
            let mut seg_texts: Vec<Vec<String>> = vec![Vec::new()];
            for t in &texts {
                if is_prot(t) {
                    seg_texts.push(Vec::new());
                } else {
                    seg_texts.last_mut().unwrap().push(t.0.clone());
                }
            }
            // words in a segment without slots move to the nearest segment that has some
            let mut carry: Vec<String> = Vec::new();
            for i in 0..seg_slots.len() {
                let mut ts = std::mem::take(&mut carry);
                ts.extend(seg_texts.get(i).cloned().unwrap_or_default());
                if seg_slots[i].is_empty() {
                    carry = ts;
                    continue;
                }
                fill_slots(items, &seg_slots[i], &ts);
            }
            if !carry.is_empty() {
                if let Some(last) = seg_slots.iter().rev().find(|s| !s.is_empty()).and_then(|s| s.last()) {
                    let prev = items[*last].out.take().unwrap_or_default();
                    let mut all = if prev.is_empty() { Vec::new() } else { vec![prev] };
                    all.extend(carry);
                    items[*last].out = Some(all.join(" "));
                }
            }
        } else {
            let ts: Vec<String> = texts.into_iter().map(|t| t.0).collect();
            fill_slots(items, idxs, &ts);
        }
    }
}

/// Distribute rendered words over the source word slots in order; surplus words join the
/// last slot, missing ones leave slots empty.
fn fill_slots(items: &mut [Item], slots: &[usize], texts: &[String]) {
    let n = slots.len();
    for (j, &k) in slots.iter().enumerate() {
        if j < texts.len() {
            if j == n - 1 && texts.len() > n {
                items[k].out = Some(texts[j..].join(" "));
            } else {
                items[k].out = Some(texts[j].clone());
            }
        } else {
            items[k].out = None;
        }
    }
}

struct Subject {
    person: Option<Person>,
    plural: bool,
    dual: bool,
    words: Vec<Word>,
    np: Option<NP>,
}

// ------------------------------------------------------------------ assembly

fn assemble(items: &[Item], _lang: &str) -> String {
    let mut out = String::new();
    let mut sent_start = true;
    let mut pending_title: Option<bool> = None;
    let mut skip_space = false;
    let mut last_was_word = false;
    for (i, it) in items.iter().enumerate() {
        match it.kind {
            Kind::Space => {
                if skip_space {
                    skip_space = false;
                    continue;
                }
                out.push_str(&it.text);
                last_was_word = false;
            }
            Kind::Punct => {
                skip_space = false;
                match &it.out {
                    Some(t) => {
                        // East Asian punctuation latinised: the trailing space is part of it
                        if t.ends_with(' ') && i + 1 < items.len() && items[i + 1].kind == Kind::Space {
                            out.push_str(t.trim_end());
                        } else {
                            out.push_str(t);
                        }
                    }
                    None => out.push_str(&it.text),
                }
                if is_sentence_end(&it.text) {
                    sent_start = true;
                    pending_title = None;
                }
                last_was_word = false;
            }
            Kind::Word | Kind::Number => {
                if sent_start {
                    // scripts without letter case (CJK, Arabic, Thai, Devanagari...) still
                    // start a Quenya sentence with a capital
                    pending_title = Some((it.cap != Cap::Lower || uncased(&it.text)) && it.kind == Kind::Word);
                    sent_start = false;
                }
                match &it.out {
                    None => {
                        // drop the space after an empty slot unless the slot was glued to the
                        // previous word (contraction piece): then that space is the only one
                        let prev_space = i > 0 && items[i - 1].kind == Kind::Space;
                        if i + 1 < items.len() && items[i + 1].kind == Kind::Space && (prev_space || !last_was_word) {
                            skip_space = true;
                        } else if out.ends_with(' ') && !last_was_word {
                            out.pop();
                        }
                    }
                    Some(t) => {
                        let mut t = t.clone();
                        if pending_title.take() == Some(true) {
                            t = title_case(&t);
                        }
                        if it.glue && last_was_word {
                            out.push(' ');
                        }
                        out.push_str(&t);
                        last_was_word = true;
                    }
                }
            }
        }
    }
    out
}
