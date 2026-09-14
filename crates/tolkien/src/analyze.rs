//! Word analysis: source word → English senses → Elvish candidates.

use crate::data::{Data, Entry, Target};
use crate::english::{self, Role};
use std::collections::HashMap;
use std::rc::Rc;
use tkdata::features as f;
use tkdata::fold::{fold_en, fold_key};
use tkdata::{link, pos};

/// Coarse syntactic category.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Cat {
    N = 0,
    V,
    Adj,
    Adv,
    Pron,
    Num,
    Prep,
    Conj,
    Intj,
    Other,
}

pub const NCAT: usize = 10;

impl Cat {
    pub fn of_pos(p: u8) -> Option<Cat> {
        Some(match p {
            pos::NOUN | pos::NAME => Cat::N,
            pos::VERB => Cat::V,
            pos::ADJ => Cat::Adj,
            pos::ADV => Cat::Adv,
            pos::PRON | pos::DET => Cat::Pron,
            pos::NUM => Cat::Num,
            pos::PREP => Cat::Prep,
            pos::CONJ => Cat::Conj,
            pos::INTERJ => Cat::Intj,
            pos::PART | pos::PHRASE | pos::CHAR | pos::OTHER | pos::UNKNOWN => Cat::Other,
            _ => return None, // suffixes / prefixes are grammar, not words
        })
    }
    pub fn idx(self) -> usize {
        self as usize
    }
}

/// One English reading of a source word.
#[derive(Clone, Debug)]
pub struct Sense {
    pub en: u32,
    pub key: String,
    pub weight: u16,
    /// Source-language part of speech (tkdata pos code, 0 = unknown).
    pub pos: u8,
    /// Alternative feature analyses (empty ⇒ base form).
    pub tagsets: Vec<u64>,
    /// Reached through a synonym link; `pos` then names the part of speech of the
    /// synonym's source page and Elvish words of another category are not candidates.
    pub alias: bool,
}

impl Sense {
    pub fn tags_any(&self) -> u64 {
        self.tagsets.iter().fold(0, |a, t| a | t)
    }
}

/// One possible Elvish rendering.
#[derive(Clone, Debug)]
pub struct Cand {
    pub entry: Entry,
    pub sense: usize,
    pub base: f32,
    pub cat: Cat,
}

#[derive(Debug, Default)]
pub struct Analysis {
    pub senses: Vec<Sense>,
    pub cands: Vec<Cand>,
    pub role: Option<Role>,
    /// All plausible grammatical roles (strongest first) with their English key and
    /// relative support in 0..=1; the tagger sees them all.
    pub roles: Vec<(Role, String, f32)>,
    /// The English lemma the role was derived from.
    pub role_key: String,
    pub cat_scores: [f32; NCAT],
    /// Features implied by English suffix guessing (when the word itself was unknown).
    pub guessed: bool,
    /// A proper name (kept as it is).
    pub name: bool,
}

impl Analysis {
    pub fn is_empty(&self) -> bool {
        self.cands.is_empty() && self.role.is_none()
    }
}

pub const CATS: [Cat; NCAT] = [Cat::N, Cat::V, Cat::Adj, Cat::Adv, Cat::Pron, Cat::Num, Cat::Prep, Cat::Conj, Cat::Intj, Cat::Other];

pub struct Analyzer<'d> {
    pub data: &'d Data,
    pub lang: String,
    pub target: Target,
    cache: HashMap<String, Rc<Analysis>>,
}

const MAX_SENSES: usize = 12;

impl<'d> Analyzer<'d> {
    pub fn new(data: &'d Data, lang: &str, target: Target) -> Self {
        Analyzer { data, lang: lang.to_string(), target, cache: HashMap::new() }
    }

    pub fn set_lang(&mut self, lang: &str) {
        if self.lang != lang {
            self.lang = lang.to_string();
            self.cache.clear();
        }
    }

    pub fn is_english(&self) -> bool {
        self.lang == "en"
    }

    /// Analyse one (possibly multi-word) source expression.
    pub fn analyze(&mut self, word: &str) -> Rc<Analysis> {
        if let Some(a) = self.cache.get(word) {
            return a.clone();
        }
        let senses = if self.is_english() { self.senses_en(word) } else { self.senses_lex(word) };
        let name = senses.is_empty() && self.known(word);
        let mut a = self.build(word, senses);
        a.name = name;
        let a = Rc::new(a);
        if self.cache.len() > 50_000 {
            self.cache.clear();
        }
        self.cache.insert(word.to_string(), a.clone());
        a
    }

    /// Analyse an English lemma directly (used for role-word fallbacks and multiword pieces).
    pub fn analyze_en(&mut self, key: &str) -> Rc<Analysis> {
        let ck = format!("\u{2}{}", key);
        if let Some(a) = self.cache.get(&ck) {
            return a.clone();
        }
        let senses = self.senses_en(key);
        let a = Rc::new(self.build(key, senses));
        self.cache.insert(ck, a.clone());
        a
    }

    /// Does the source lexicon know this (folded) word?
    pub fn known(&self, word: &str) -> bool {
        if self.is_english() {
            self.data.en_id(&fold_en(word)).is_some()
        } else {
            self.data.lex_has(&self.lang, &fold_key(&self.lang, word))
        }
    }

    fn push_sense(senses: &mut Vec<Sense>, s: Sense) {
        if let Some(e) = senses.iter_mut().find(|e| e.en == s.en) {
            if s.weight > e.weight {
                e.weight = s.weight;
            }
            for t in s.tagsets {
                if !e.tagsets.contains(&t) {
                    e.tagsets.push(t);
                }
            }
            if e.pos == 0 {
                e.pos = s.pos;
            }
        } else if senses.len() < MAX_SENSES * 2 {
            senses.push(s);
        }
    }

    fn senses_en(&self, word: &str) -> Vec<Sense> {
        let key = fold_en(word);
        let mut senses = Vec::new();
        // single letters (variables, list markers) other than a / I / O are not words
        if key.len() == 1 && key.is_ascii() && !matches!(key.as_str(), "a" | "i" | "o") {
            return senses;
        }
        let acronym = (2..=4).contains(&word.len()) && word.bytes().all(|b| b.is_ascii_uppercase());
        if let Some((lemma, tags)) = english::irregular(&key) {
            if let Some(id) = self.data.en_id(lemma) {
                self.add_en_id(&mut senses, id, lemma, 100, vec![tags], 0);
                if !senses.is_empty() {
                    return senses;
                }
            }
        }
        if let Some(id) = self.data.en_id(&key) {
            self.add_en_id(&mut senses, id, &key, 100, vec![0], 0);
        }
        if senses.is_empty() || senses.iter().all(|s| s.weight < 90) {
            for g in english::guess_lemmas(&key) {
                if let Some(id) = self.data.en_id(&g.lemma) {
                    let mut ts = vec![g.tags];
                    if g.tags == 0 {
                        ts = vec![0];
                    }
                    self.add_en_id(&mut senses, id, &g.lemma, 78, ts, g.pos);
                }
            }
        }
        if acronym {
            // IO, API, PR ...: keep only direct senses or a plain plural/3sg of a known lemma
            let plural = key.strip_suffix('s').map(|s| s.to_string());
            senses.retain(|s| s.weight >= 90 || (!s.alias && plural.as_deref() == Some(s.key.as_str())));
        }
        // a guessed stem never turns a content word into a function word ("Turkish" → from)
        if senses.iter().all(|s| s.weight <= 78) {
            senses.retain(|s| english::role(&s.key).map(|r| !english::is_function_role(&r)).unwrap_or(true));
        }
        senses.sort_by(|a, b| b.weight.cmp(&a.weight));
        senses.truncate(MAX_SENSES);
        Self::drop_name_glosses(word, &mut senses);
        senses
    }

    /// Add an English id and (for form links) its lemmas.
    fn add_en_id(&self, senses: &mut Vec<Sense>, id: u32, key: &str, weight: u16, tagsets: Vec<u64>, pos_hint: u8) {
        let e = self.data.en_entry(id);
        let direct = !e.quenya.is_empty() || !e.sindarin.is_empty();
        if direct {
            Self::push_sense(senses, Sense { en: id, key: key.to_string(), weight, pos: pos_hint, tagsets: tagsets.clone(), alias: false });
        }
        for l in &e.links {
            let mut w = (l.weight as u16).min(weight);
            let mut tags = l.tags;
            let mut pos = pos_hint;
            let alias = l.kind == link::ALIAS;
            if alias {
                w = w.saturating_sub(10);
                if pos == 0 {
                    pos = match tags & f::POS_ANY {
                        f::POS_N => pos::NOUN,
                        f::POS_V => pos::VERB,
                        f::POS_ADJ => pos::ADJ,
                        f::POS_ADV => pos::ADV,
                        _ => 0,
                    };
                }
                tags = 0;
            }
            if tags & f::ALT != 0 {
                w = w.saturating_sub(25);
            }
            if tags & f::ARCHAIC != 0 {
                w = w.saturating_sub(12);
            }
            if tags & f::FORMOF != 0 {
                w = w.saturating_sub(3);
            }
            let inherited: Vec<u64> = if tagsets.iter().all(|t| *t == 0) {
                vec![tags & !(f::ALT | f::ARCHAIC | f::FORMOF)]
            } else {
                tagsets.iter().map(|t| t | (tags & !(f::ALT | f::ARCHAIC | f::FORMOF))).collect()
            };
            let tkey = self.data.en_key(l.target);
            Self::push_sense(senses, Sense { en: l.target, key: tkey.clone(), weight: w, pos, tagsets: inherited.clone(), alias });
            if alias {
                continue;
            }
            // "waits" → wait: a lemma Elvish does not cover lends its synonyms to its forms
            let te = self.data.en_entry(l.target);
            if w >= 85 && !self.has_elvish(l.target) {
                for l2 in te.links.iter().filter(|l2| l2.kind == link::ALIAS) {
                    let w2 = (l2.weight as u16).min(w).saturating_sub(10);
                    let pos2 = match l2.tags & f::POS_ANY {
                        f::POS_N => pos::NOUN,
                        f::POS_V => pos::VERB,
                        f::POS_ADJ => pos::ADJ,
                        f::POS_ADV => pos::ADV,
                        _ => pos,
                    };
                    let k2 = self.data.en_key(l2.target);
                    Self::push_sense(senses, Sense { en: l2.target, key: k2, weight: w2, pos: pos2, tagsets: inherited.clone(), alias: true });
                }
            }
        }
    }

    /// An inflected form missing from the lexicon: the nearest lemma sharing a long stem
    /// ("wschodzi" → wschodzić). Conservative: at most four letters differ at the end.
    fn stem_fallback(&self, folded: &str) -> Option<String> {
        let chars: Vec<char> = folded.chars().collect();
        let n = chars.len();
        if n < 5 || chars.iter().any(|c| !c.is_alphabetic()) {
            return None;
        }
        let min_cut = (n - 4).max(5).max((n * 3 + 4) / 5);
        for cut in (min_cut..n).rev() {
            let prefix: String = chars[..cut].iter().collect();
            let keys = self.data.lex_prefix_keys(&self.lang, &prefix, 48);
            let mut scored: Vec<(i64, String)> = Vec::new();
            for k in keys {
                let kl = k.chars().count();
                if k.contains(' ') || kl > n + 4 || kl < 4 {
                    continue;
                }
                // prefer lemma-like (short) keys close in length to the word
                let score = -(kl as i64 - cut as i64).abs() * 2 - (kl as i64 - n as i64).abs();
                scored.push((score, k));
            }
            if scored.is_empty() {
                continue;
            }
            scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
            let top = scored[0].0;
            // among near-ties prefer a dictionary lemma over another inflected form
            let mut best: Option<(i32, String)> = None;
            for (sc, k) in scored.iter().take(6) {
                if *sc < top - 1 {
                    break;
                }
                let lemma = self
                    .data
                    .lex_lookup(&self.lang, k)
                    .map(|cs| {
                        let infl = f::CASES | f::NUMBER | f::PERSONS | f::PAST | f::FUT | f::PERF | f::MOODS | f::DEGREES | f::FORMOF;
                        cs.iter().all(|c| c.tagsets.iter().all(|&t| t & infl == 0))
                    })
                    .unwrap_or(false);
                let v = (*sc as i32) * 2 + if lemma { 3 } else { 0 };
                if best.as_ref().map(|(bv, _)| v > *bv).unwrap_or(true) {
                    best = Some((v, k.clone()));
                }
            }
            return best.map(|(_, k)| k);
        }
        None
    }

    fn senses_lex(&self, word: &str) -> Vec<Sense> {
        let folded = fold_key(&self.lang, word);
        let mut senses = Vec::new();
        let mut looked = self.data.lex_lookup(&self.lang, &folded);
        // います is a polite form of いる, whatever stray entry Wiktionary keeps under it
        let polite_ja = self.lang == "ja" && ["ます", "ました", "ません", "ませんでした"].iter().any(|e| folded.ends_with(e));
        if polite_ja && looked.as_ref().map(|cs| cs.iter().all(|c| c.tagsets.iter().all(|&t| t & f::TENSES == 0))).unwrap_or(false) {
            looked = None;
        }
        if looked.is_none() {
            // suffix rules verified against the lexicon ("dao" → dati + past)
            for (lemma, tags) in crate::morph::guesses(&self.lang, &folded) {
                if let Some(cs) = self.data.lex_lookup(&self.lang, &lemma) {
                    let verbal = tags & (f::TENSES | f::PERSONS) != 0;
                    let fits: Vec<crate::data::LexCand> = cs
                        .into_iter()
                        .filter(|c| c.pos == 0 || !verbal || c.pos == pos::VERB)
                        .map(|mut c| {
                            c.tagsets = vec![tags];
                            c.weight = c.weight.saturating_sub(3);
                            c
                        })
                        .collect();
                    if !fits.is_empty() {
                        looked = Some(fits);
                        break;
                    }
                }
            }
        }
        if looked.is_none() && !word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            if let Some(stem) = self.stem_fallback(&folded) {
                looked = self.data.lex_lookup(&self.lang, &stem).map(|cs| {
                    cs.into_iter()
                        .map(|mut c| {
                            // the lemma's own tags do not describe this form
                            c.tagsets.clear();
                            c.weight = c.weight.saturating_sub(6);
                            c
                        })
                        .collect()
                });
            }
        }
        if let Some(cands) = looked {
            for c in cands {
                let key = self.data.en_key(c.en);
                let mut tagsets = if c.tagsets.is_empty() { vec![0] } else { c.tagsets.clone() };
                // noisy inflected keys ("is", "men"): remap to the lemma
                if let Some((lemma, tags)) = english::irregular(&key) {
                    if let Some(id) = self.data.en_id(lemma) {
                        if tags != 0 {
                            tagsets = tagsets.iter().map(|t| t | tags).collect();
                        }
                        // "don't" → not is a particle, whatever the source section said
                        let p = if lemma == "not" { pos::PART } else { c.pos };
                        self.add_en_id(&mut senses, id, lemma, c.weight as u16, tagsets, p);
                        continue;
                    }
                }
                // the English key itself may be an inflected form ("men", "ate")
                let e = self.data.en_entry(c.en);
                if e.quenya.is_empty() && e.sindarin.is_empty() && !e.links.is_empty() {
                    self.add_en_id(&mut senses, c.en, &key, c.weight as u16, tagsets, c.pos);
                } else {
                    Self::push_sense(&mut senses, Sense { en: c.en, key, weight: c.weight as u16, pos: c.pos, tagsets, alias: false });
                }
            }
        }
        senses.sort_by(|a, b| b.weight.cmp(&a.weight));
        senses.truncate(MAX_SENSES);
        Self::drop_name_glosses(word, &mut senses);
        if matches!(self.lang.as_str(), "ja" | "ko") {
            // an inflected form is never also the bare stem: keep the tensed readings
            for s in senses.iter_mut() {
                if s.tagsets.iter().any(|&t| t & (f::TENSES | f::NEG) != 0) {
                    s.tagsets.retain(|&t| t & (f::TENSES | f::NEG) != 0);
                }
            }
        }
        senses
    }

    /// A proper name's gloss words ("Warsaw: the capital of Poland" → capital, city) leak
    /// into the lexicon as weak senses; the name itself should stay as it is.
    fn drop_name_glosses(word: &str, senses: &mut Vec<Sense>) {
        if senses.is_empty() {
            return;
        }
        let upper = word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
        let top_name = senses[0].pos == pos::NAME;
        let all_weak = senses.iter().all(|s| s.weight <= 55);
        if top_name || (upper && all_weak) {
            senses.clear();
        }
    }

    fn has_elvish(&self, en: u32) -> bool {
        let e = self.data.en_entry(en);
        match self.target {
            Target::Quenya => !e.quenya.is_empty(),
            Target::Sindarin => !e.sindarin.is_empty(),
        }
    }

    fn build(&self, word: &str, mut senses: Vec<Sense>) -> Analysis {
        // lemmas without an Elvish rendering fall back to a covered near-synonym
        for s in senses.iter_mut() {
            if !self.has_elvish(s.en) {
                if let Some(alt) = english::synonym(&s.key) {
                    if alt != s.key {
                        if let Some(id) = self.data.en_id(alt) {
                            if self.has_elvish(id) {
                                s.en = id;
                                s.key = alt.to_string();
                            }
                        }
                    }
                }
            }
        }
        // support = lexical weight + frequency tie-breaker + agreement between senses
        // on the same Elvish entry − penalty for gender-derived glosses ("reina" → king (fem))
        let elvish_ids: Vec<Vec<u32>> = senses
            .iter()
            .map(|s| {
                let e = self.data.en_entry(s.en);
                let list = match self.target {
                    Target::Quenya => &e.quenya,
                    Target::Sindarin => &e.sindarin,
                };
                list.iter().take(6).map(|(id, _)| *id).collect()
            })
            .collect();
        let mut support: Vec<f32> = Vec::with_capacity(senses.len());
        for (i, s) in senses.iter().enumerate() {
            let mut sup = s.weight as f32 + english::freq_bonus(&s.key);
            // translation links confirmed by an early gloss of the source page are the
            // primary meanings (91-94; 88-90 for their inflected forms)
            sup += if s.weight >= 91 { 6.0 } else if s.weight >= 88 { 3.0 } else { 0.0 };
            let mut agree = 0;
            for (j, other) in elvish_ids.iter().enumerate() {
                if j != i && (senses[j].weight as f32) >= s.weight as f32 * 0.8 && other.iter().any(|id| elvish_ids[i].contains(id)) {
                    agree += 1;
                }
            }
            // convergence on one Elvish entry supports content readings; function words
            // ("at"/"in" both → se) gain nothing from it
            if english::role(&s.key).map(|r| !english::is_function_role(&r)).unwrap_or(true) {
                sup += 3.0 * (agree.min(3) as f32);
            }
            if !self.is_english() && s.pos == pos::NOUN && s.tagsets.iter().any(|t| t & (f::FEM | f::MASC) != 0) {
                sup -= 3.0;
            }
            support.push(sup);
        }
        let mut order: Vec<usize> = (0..senses.len()).collect();
        order.sort_by(|&x, &y| {
            support[y]
                .partial_cmp(&support[x])
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| english::freq_rank(&senses[x].key).unwrap_or(usize::MAX).cmp(&english::freq_rank(&senses[y].key).unwrap_or(usize::MAX)))
        });
        let senses_sorted: Vec<Sense> = order.iter().map(|&i| senses[i].clone()).collect();
        let support_sorted: Vec<f32> = order.iter().map(|&i| support[i]).collect();
        senses = senses_sorted;
        let mut a = Analysis { senses, ..Default::default() };
        let max_sup = support_sorted.first().copied().unwrap_or(1.0).max(1.0);
        // English: the surface word decides its own grammatical role
        if self.is_english() && !word.contains(' ') {
            let key = fold_en(word);
            let (rk, r) = match english::irregular(&key) {
                Some((lemma, _)) => (lemma.to_string(), english::role(lemma)),
                None => (key.clone(), english::role(&key)),
            };
            if let Some(r) = r {
                a.role = Some(r);
                a.role_key = rk.clone();
                a.roles.push((r, rk.clone(), 1.0));
                // "her" is both the possessive determiner and the object pronoun
                if rk == "her" {
                    a.roles.push((Role::Pronoun { p: crate::quenya::Person::S3, obj: true, refl: false }, rk, 1.0));
                }
            }
        }
        // grammatical roles from the strong senses (all readings of comparable lexical
        // weight; the tagger decides by context)
        if a.roles.is_empty() {
            let max_w = a.senses.iter().map(|s| s.weight).max().unwrap_or(0) as f32;
            for (i, s) in a.senses.iter().enumerate() {
                if (s.weight as f32) < max_w * 0.8 {
                    continue;
                }
                if let Some(mut r) = english::role(&s.key) {
                    // a guessed English stem never makes a function word ("Turkish" → from)
                    if self.is_english() && max_w <= 78.0 && english::is_function_role(&r) {
                        continue;
                    }
                    // a conjunction glossed "that" (dass, że) is the complementizer, not the
                    // demonstrative; a pronoun/adverb glossed "that" (que, che, что) is both
                    if s.key == "that" && !self.is_english() {
                        if s.pos == pos::CONJ {
                            r = Role::Conj("sa");
                        } else if matches!(s.pos, pos::PRON | pos::ADV)
                            && !a.roles.iter().any(|(er, _, _)| *er == Role::Conj("sa"))
                            && !a.senses.iter().any(|o| matches!(o.key.as_str(), "the" | "this" | "these" | "those"))
                        {
                            a.roles.push((Role::Conj("sa"), s.key.clone(), 1.0));
                        }
                    }
                    // for foreign words prefer role readings only when the source POS agrees
                    // (the section a translation-table link sits under is unreliable: "when"
                    // links from its noun section too; trust confirmed links regardless)
                    let ok = (self.is_english()
                        || s.pos == 0
                        || s.weight >= 90
                        || matches!(s.pos, pos::PRON | pos::DET | pos::PREP | pos::CONJ | pos::PART | pos::ADV | pos::NUM | pos::INTERJ)
                        || matches!(r, Role::Copula | Role::Aux(_) | Role::Neg)
                        // 什么 "what" sits under a noun heading; a top-ranked question word is one
                        || (matches!(r, Role::Wh(_)) && s.weight as f32 >= max_w))
                        && !(matches!(r, Role::IndefArt) && s.pos == pos::NOUN && !self.is_english())
                        // a number/gender-inflected reading is a noun form, not a negation particle
                        // (Spanish "nos" = plural of the noun "no")
                        && !(matches!(r, Role::Neg) && !self.is_english() && !s.tagsets.is_empty() && s.tagsets.iter().all(|t| t & (f::NUMBER | f::GENDERS) != 0));
                    if ok && !a.roles.iter().any(|(er, _, _)| *er == r) {
                        // verbal function roles depend on lexical support ("mam" = have, not be);
                        // nominal ones are decided by context alone
                        // an alternative-spelling reading loses to a real content reading of the
                        // same strength ("deu" ~ "de" vs. deu = gave); contractions ("à") keep theirs
                        let alt_only = !s.tagsets.is_empty()
                            && s.tagsets.iter().all(|t| t & f::ALT != 0)
                            && a.senses.iter().any(|o| {
                                matches!(o.pos, pos::NOUN | pos::VERB | pos::ADJ)
                                    && o.weight as f32 >= s.weight as f32 - 5.0
                                    && !o.tagsets.iter().all(|t| t & f::ALT != 0)
                            });
                        let mut rel = if matches!(r, Role::Copula | Role::Aux(_)) {
                            (support_sorted[i] / max_sup).powi(4)
                        } else {
                            // a clearly weaker secondary reading ("kiedy" → as 81 vs when 94) stays
                            // available but does not outweigh the primary one in a neutral context
                            let w = s.weight as f32;
                            if w >= max_w - 1.0 {
                                1.0
                            } else if w >= max_w * 0.9 {
                                0.9
                            } else {
                                0.8
                            }
                        };
                        if alt_only {
                            // a mere alternative spelling of some other word ("deu" ~ "de")
                            rel *= 0.6;
                        }
                        a.roles.push((r, s.key.clone(), rel));
                    }
                }
            }
            // "don't"-derived "do" readings of foreign negation particles
            if a.roles.iter().any(|(r, _, _)| matches!(r, Role::Neg)) {
                a.roles.retain(|(r, _, _)| !matches!(r, Role::Aux(english::Aux::Do)));
            }
            // a foreign verb is either "be" or "have": keep the verbal role(s) of top weight only
            if !self.is_english() {
                let wt = |key: &str| a.senses.iter().find(|s| s.key == key).map(|s| s.weight).unwrap_or(0);
                let max_vw = a.roles.iter().filter(|(r, _, _)| matches!(r, Role::Copula | Role::Aux(_))).map(|(_, k, _)| wt(k)).max().unwrap_or(0);
                if max_vw > 0 {
                    a.roles.retain(|(r, k, _)| !matches!(r, Role::Copula | Role::Aux(_)) || wt(k) + 1 >= max_vw);
                }
            }
            // object-only pronoun readings are weaker (Polish "nie" = "them" is rare) when the
            // word has a non-pronominal reading
            let has_other = a.roles.iter().any(|(r, _, _)| !matches!(r, Role::Pronoun { .. }));
            // Romance articles double as object clitics ("lo veo", "je le vois", "la vedo"):
            // such a pronoun reading is never the subject
            let article_clitic = !self.is_english()
                && matches!(self.lang.as_str(), "es" | "pt" | "it" | "fr" | "ca" | "gl" | "ro" | "oc" | "ast" | "an" | "sc" | "co" | "nap" | "scn" | "vec" | "lad" | "fur" | "rm" | "la")
                && a.roles.iter().any(|(r, _, _)| matches!(r, Role::DefArt | Role::PossDet(_)));
            let mut persons_plain: Vec<crate::quenya::Person> = Vec::new();
            for (r, key, rel) in a.roles.iter_mut() {
                if let Role::Pronoun { p, obj, refl } = r {
                    if let Some(s) = a.senses.iter().find(|s| s.key == *key) {
                        if has_other && !s.tagsets.is_empty() && s.tagsets.iter().all(|t| t & f::ACC != 0) && s.tagsets.iter().all(|t| t & f::NOM == 0) {
                            *rel = 0.8;
                        }
                        // "cię", "go", "ihn", "lo": a form whose case readings are all oblique is an
                        // object pronoun even though its English key is the nominative lemma
                        let cased: Vec<u64> = s.tagsets.iter().copied().filter(|t| t & f::CASES != 0).collect();
                        if !self.is_english() && !cased.is_empty() && cased.iter().all(|t| t & f::NOM == 0) {
                            *obj = true;
                        }
                        if article_clitic {
                            *obj = true;
                        }
                        // plural forms ("was", "vous", "les", "euch")
                        if !s.tagsets.is_empty() && s.tagsets.iter().all(|t| t & f::PL != 0) {
                            use crate::quenya::Person::*;
                            *p = match *p {
                                S1 => P1Ex,
                                S2Pol | S2Fam => P2,
                                S3 => P3,
                                S3Inan => P3Inan,
                                other => other,
                            };
                        }
                    }
                    if !*refl {
                        persons_plain.push(*p);
                    }
                }
            }
            // "te", "me", "nos": the reflexive reading yields to the plain object pronoun
            if !self.is_english() {
                for (r, _, rel) in a.roles.iter_mut() {
                    if let Role::Pronoun { p, refl: true, .. } = r {
                        if persons_plain.contains(p) {
                            *rel *= 0.9;
                        }
                    }
                }
                // "mu", "jej": a weaker demonstrative reading of a strong oblique personal
                // pronoun is a dictionary artefact, not a determiner
                let strong_obj = a.roles.iter().any(|(r, _, rel)| matches!(r, Role::Pronoun { obj: true, refl: false, .. }) && *rel >= 0.999);
                if strong_obj {
                    a.roles.retain(|(r, _, rel)| !(matches!(r, Role::Demonstrative { .. }) && *rel < 0.999));
                }
            }
            if let Some((r, k, _)) = a.roles.first() {
                a.role = Some(*r);
                a.role_key = k.clone();
            }
        }
        let sense_support = support_sorted;
        // candidates
        let input_cap = word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
        for (si, s) in a.senses.iter().enumerate() {
            let e = self.data.en_entry(s.en);
            let list = match self.target {
                Target::Quenya => &e.quenya,
                Target::Sindarin => &e.sindarin,
            };
            let artefact = english::is_artefact(&s.key);
            let pref = english::preferred(&s.key);
            for &(qid, qscore) in list.iter().take(8) {
                let Some(entry) = self.data.elvish(self.target, qid) else { continue };
                let Some(cat) = Cat::of_pos(entry.pos) else { continue };
                // the primary readings of the word dominate; weaker senses fall off quickly
                // (an English inflected form is as certain as the base word; the tagger decides)
                let rel = if self.is_english() && s.weight >= 90 { 1.0 } else { (sense_support[si] / max_sup).min(1.0) };
                let mut base = qscore as f32 * (0.55 + 0.7 * rel.powi(6));
                if artefact {
                    base -= 60.0;
                }
                if let Some(scat) = Cat::of_pos(s.pos) {
                    if s.pos != 0 && scat != cat && matches!(scat, Cat::N | Cat::V | Cat::Adj | Cat::Adv) {
                        // (an adverb is formed from the adjective: "typically" → typical → -vë)
                        if s.alias && matches!(cat, Cat::N | Cat::V | Cat::Adj | Cat::Adv) && !(scat == Cat::Adv && cat == Cat::Adj) {
                            continue;
                        }
                        base -= 20.0;
                    }
                }
                let form = entry.form.trim_end_matches('-');
                let capitalised = form.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
                if capitalised && entry.pos != pos::NAME && !input_cap {
                    base -= 8.0;
                }
                if entry.pos == pos::NAME && !input_cap {
                    // a proper name never renders a lower-case adjective/verb ("powerful" ≠ Turcafinwë)
                    let non_nominal = matches!(Cat::of_pos(s.pos), Some(Cat::V | Cat::Adj | Cat::Adv));
                    base -= if non_nominal { 40.0 } else { 12.0 };
                }
                if entry.pos == pos::PHRASE {
                    base -= 10.0;
                }
                if form.contains(' ') {
                    base -= 8.0;
                }
                let any = s.tags_any();
                if any & f::ALT != 0 && s.tagsets.iter().all(|t| t & f::ALT != 0) {
                    // a dialectal/variant reading ("딸" ~ 달 moon) loses to a real one (daughter)
                    let rival = a.senses.iter().any(|o| {
                        matches!(o.pos, pos::NOUN | pos::VERB | pos::ADJ)
                            && o.weight as f32 >= s.weight as f32 - 5.0
                            && !o.tagsets.iter().all(|t| t & f::ALT != 0)
                    });
                    base -= if rival { 30.0 } else { 15.0 };
                }
                if let Some(p) = pref {
                    if p == entry.form || p == form {
                        base += 30.0;
                    }
                }
                if base > a.cat_scores[cat.idx()] {
                    a.cat_scores[cat.idx()] = base;
                }
                a.cands.push(Cand { entry, sense: si, base, cat });
            }
        }
        // an article is only an article ("a" → of/about → pá is a dictionary artefact)
        if a.roles.first().map(|(r, _, rel)| matches!(r, Role::DefArt | Role::IndefArt) && *rel >= 0.999).unwrap_or(false) {
            a.cands.clear();
            a.cat_scores = [0.0; NCAT];
        }
        a.cands.sort_by(|x, y| y.base.partial_cmp(&x.base).unwrap_or(std::cmp::Ordering::Equal));
        a.cands.truncate(24);
        // a confidently identified word whose meaning Elvish does not cover stays untranslated
        // rather than becoming a remote secondary sense ("Hobbit" → "fantasy" → land)
        if let Some(top) = a.senses.first() {
            let top_has = a.cands.iter().any(|c| c.sense == 0);
            if !top_has && top.weight >= 85 && !a.cands.is_empty() {
                // near synonyms ("error" → err, mistake) stay; remote ones ("error" → show) go
                let thr = 0.65 * top.weight as f32;
                let senses = &a.senses;
                a.cands.retain(|c| (senses[c.sense].weight as f32) >= thr);
                a.cat_scores = [0.0; NCAT];
                if a.cands.is_empty() {
                    if let Some(c) = Cat::of_pos(top.pos) {
                        a.cat_scores[c.idx()] = 100.0;
                    }
                } else {
                    for c in &a.cands {
                        if c.base > a.cat_scores[c.cat.idx()] {
                            a.cat_scores[c.cat.idx()] = c.base;
                        }
                    }
                }
            }
        }
        a.guessed = a.senses.iter().all(|s| s.weight <= 78) && !a.senses.is_empty();
        a
    }
}
