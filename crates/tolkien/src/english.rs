//! English interlingua: grammatical roles of function words, contractions,
//! and a morphological fallback for words missing from the dictionary.

use crate::quenya::{Case, Person};
use tkdata::features as f;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Aux {
    /// have/has/had + participle
    Perfect,
    /// will / shall
    Future,
    /// would / should
    Cond,
    /// can / could
    Can,
    /// may / might
    May,
    /// must / ought / need
    Must,
    /// do / does / did (emphasis, negation, questions)
    Do,
    /// let (let us go)
    Let,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PrepRole {
    /// Rendered as a noun case on the following phrase.
    Case(Case),
    /// Quenya preposition word followed by the nominative.
    Word(&'static str),
    /// Quenya preposition governing the genitive.
    WordGen(&'static str),
    /// "with" in the sense of accompaniment: as + pronoun, instrumental otherwise.
    With,
    /// "than" in comparisons.
    Than,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    DefArt,
    IndefArt,
    /// this/that (far = false/true), plural these/those
    Demonstrative { far: bool, plural: bool },
    PossDet(Person),
    Pronoun { p: Person, obj: bool, refl: bool },
    /// Any form of "to be"; tense comes from the form tags.
    Copula,
    Aux(Aux),
    Neg,
    Prep(PrepRole),
    /// Conjunction rendered by the given Quenya word.
    Conj(&'static str),
    /// Interrogative / relative word rendered by the given Quenya word.
    Wh(&'static str),
    Number(u64),
    Ordinal(u64),
    /// Prenominal quantifier rendered as an adjective (agrees in number).
    Quant(&'static str),
    /// Existential "there".
    There,
    /// Fixed translation (adverbs, interjections, particles).
    Word(&'static str),
}

/// Grammatical roles of English function words (keys are folded lemmas).
pub fn role(key: &str) -> Option<Role> {
    use Role::*;
    Some(match key {
        "the" => DefArt,
        "a" | "an" => IndefArt,
        "this" => Demonstrative { far: false, plural: false },
        "these" => Demonstrative { far: false, plural: true },
        "that" => Demonstrative { far: true, plural: false },
        "those" => Demonstrative { far: true, plural: true },
        "my" | "mine" => PossDet(Person::S1),
        "your" | "yours" | "thy" | "thine" => PossDet(Person::S2Pol),
        "his" => PossDet(Person::S3),
        "her" | "hers" => PossDet(Person::S3),
        "its" => PossDet(Person::S3Inan),
        "our" | "ours" => PossDet(Person::P1Ex),
        "their" | "theirs" => PossDet(Person::P3),
        "i" => Pronoun { p: Person::S1, obj: false, refl: false },
        "me" => Pronoun { p: Person::S1, obj: true, refl: false },
        "myself" => Pronoun { p: Person::S1, obj: true, refl: true },
        "you" | "ye" => Pronoun { p: Person::S2Pol, obj: false, refl: false },
        "thou" => Pronoun { p: Person::S2Fam, obj: false, refl: false },
        "thee" => Pronoun { p: Person::S2Fam, obj: true, refl: false },
        "yourself" | "thyself" => Pronoun { p: Person::S2Pol, obj: true, refl: true },
        "yourselves" => Pronoun { p: Person::P2, obj: true, refl: true },
        "he" => Pronoun { p: Person::S3, obj: false, refl: false },
        "him" => Pronoun { p: Person::S3, obj: true, refl: false },
        "himself" | "herself" | "oneself" => Pronoun { p: Person::S3, obj: true, refl: true },
        "she" => Pronoun { p: Person::S3, obj: false, refl: false },
        "it" => Pronoun { p: Person::S3Inan, obj: false, refl: false },
        "itself" => Pronoun { p: Person::S3Inan, obj: true, refl: true },
        "we" => Pronoun { p: Person::P1Ex, obj: false, refl: false },
        "us" => Pronoun { p: Person::P1Ex, obj: true, refl: false },
        "ourselves" => Pronoun { p: Person::P1Ex, obj: true, refl: true },
        "they" => Pronoun { p: Person::P3, obj: false, refl: false },
        "them" => Pronoun { p: Person::P3, obj: true, refl: false },
        "themselves" => Pronoun { p: Person::P3, obj: true, refl: true },
        "be" => Copula,
        "have" => Aux(self::Aux::Perfect),
        "will" | "shall" => Aux(self::Aux::Future),
        "would" | "should" | "ought" => Aux(self::Aux::Cond),
        "can" => Aux(self::Aux::Can),
        "may" | "might" => Aux(self::Aux::May),
        "must" | "need" => Aux(self::Aux::Must),
        "do" => Aux(self::Aux::Do),
        "let" => Aux(self::Aux::Let),
        "not" | "n't" => Neg,
        "of" => Prep(PrepRole::Case(Case::Gen)),
        "to" | "into" | "onto" | "toward" | "towards" | "unto" => Prep(PrepRole::Case(Case::All)),
        "in" | "at" | "on" | "inside" | "within" | "upon" | "during" => Prep(PrepRole::Case(Case::Loc)),
        "from" | "off" | "since" => Prep(PrepRole::Case(Case::Abl)),
        "by" | "through" | "via" | "per" => Prep(PrepRole::Case(Case::Ins)),
        "with" => Prep(PrepRole::With),
        "for" => Prep(PrepRole::Case(Case::Dat)),
        "without" => Prep(PrepRole::WordGen("ú")),
        "about" | "concerning" | "regarding" => Prep(PrepRole::Word("pá")),
        "under" | "beneath" | "below" => Prep(PrepRole::Word("nu")),
        "over" | "above" => Prep(PrepRole::Word("or")),
        "among" | "amongst" | "between" | "amid" => Prep(PrepRole::Word("imbë")),
        "before" => Prep(PrepRole::Word("nó")),
        "after" => Prep(PrepRole::Word("apa")),
        "behind" => Prep(PrepRole::Word("ca")),
        "beside" | "next" | "near" | "along" => Prep(PrepRole::Word("ara")),
        "against" => Prep(PrepRole::Word("ana")),
        "around" | "round" => Prep(PrepRole::Word("os")),
        "across" | "beyond" | "past" => Prep(PrepRole::Word("pella")),
        "until" | "till" => Prep(PrepRole::Word("tenna")),
        "like" | "as" => Prep(PrepRole::Word("ve")),
        "than" => Prep(PrepRole::Than),
        "despite" | "notwithstanding" => Prep(PrepRole::Word("ómu")),
        "and" => Conj("ar"),
        "or" => Conj("hya"),
        "nor" => Conj("ar lá"),
        "but" | "yet" | "however" => Conj("mal"),
        "because" => Conj("pan"),
        "if" | "whether" => Conj("qui"),
        "unless" => Conj("qui lá"),
        "so" | "therefore" | "thus" | "hence" => Conj("sië"),
        "then" => Conj("tá"),
        "although" | "though" | "albeit" => Conj("ómu"),
        "while" | "whilst" => Conj("írë"),
        "when" | "whenever" => Wh("yá"),
        "where" | "wherever" => Wh("massë"),
        "why" => Wh("manan"),
        "how" => Wh("manen"),
        "who" | "whom" | "whoever" => Wh("man"),
        "whose" => Wh("mano"),
        "what" | "whatever" => Wh("mana"),
        "which" | "whichever" => Wh("ya"),
        "zero" => Number(0),
        "one" => Number(1),
        "two" => Number(2),
        "three" => Number(3),
        "four" => Number(4),
        "five" => Number(5),
        "six" => Number(6),
        "seven" => Number(7),
        "eight" => Number(8),
        "nine" => Number(9),
        "ten" => Number(10),
        "eleven" => Number(11),
        "twelve" => Number(12),
        "thirteen" => Number(13),
        "fourteen" => Number(14),
        "fifteen" => Number(15),
        "sixteen" => Number(16),
        "seventeen" => Number(17),
        "eighteen" => Number(18),
        "nineteen" => Number(19),
        "twenty" => Number(20),
        "thirty" => Number(30),
        "forty" => Number(40),
        "fifty" => Number(50),
        "sixty" => Number(60),
        "seventy" => Number(70),
        "eighty" => Number(80),
        "ninety" => Number(90),
        "hundred" => Number(100),
        "thousand" => Number(1000),
        "million" => Number(1_000_000),
        "first" => Ordinal(1),
        "second" => Ordinal(2),
        "third" => Ordinal(3),
        "fourth" => Ordinal(4),
        "fifth" => Ordinal(5),
        "sixth" => Ordinal(6),
        "seventh" => Ordinal(7),
        "eighth" => Ordinal(8),
        "ninth" => Ordinal(9),
        "tenth" => Ordinal(10),
        "eleventh" => Ordinal(11),
        "twelfth" => Ordinal(12),
        "all" | "every" | "each" | "whole" | "entire" => Quant("ilya"),
        "some" | "several" | "certain" => Quant("uma"),
        "many" | "much" | "lot" | "lots" | "plenty" | "numerous" => Quant("rimba"),
        "few" | "little" => Quant("nótima"),
        "no" | "none" => Quant("ú"),
        "any" => Quant("aiqua"),
        "other" | "another" | "else" => Quant("hyana"),
        "same" => Quant("imya"),
        "such" => Quant("taitë"),
        "own" => Quant("véra"),
        "more" => Quant("amba"),
        "most" => Quant("ambe"),
        "both" => Quant("yúyo"),
        "there" => There,
        "here" => Word("sinomë"),
        "today" => Word("síra"),
        "tonight" => Word("silómë"),
        "tomorrow" => Word("enar"),
        "yesterday" => Word("noa"),
        "home" => Word("mardenna"),
        "away" => Word("oa"),
        "back" => Word("nan"),
        "together" => Word("uo"),
        "alone" => Word("eressë"),
        "almost" => Word("harë"),
        "quite" | "rather" => Word("farë"),
        "even" => Word("yando"),
        "now" => Word("sí"),
        "very" => Word("ita"),
        "too" => Word("yú"),
        "also" => Word("yú"),
        "only" | "just" | "merely" => Word("rië"),
        "again" => Word("ata"),
        "never" => Word("úlumë"),
        "always" | "ever" | "forever" => Word("illumë"),
        "often" => Word("rimbavë"),
        "sometimes" => Word("umbë"),
        "already" => Word("yando"),
        "still" => Word("er"),
        "soon" => Word("rato"),
        "perhaps" | "maybe" => Word("cé"),
        "yes" | "yeah" | "yep" | "indeed" => Word("ná"),
        "hello" | "hi" | "hey" | "greetings" => Word("aiya"),
        "goodbye" | "bye" | "farewell" => Word("namárië"),
        "thanks" => Word("hantalë"),
        "please" => Word("mecin"),
        "okay" | "ok" | "alright" => Word("mai"),
        "well" => Word("mai"),
        "oh" | "ah" | "alas" => Word("ai"),
        "behold" | "lo" => Word("yé"),
        "amen" => Word("násië"),
        "nothing" => Word("munta"),
        "nobody" => Word("úquen"),
        "something" | "anything" => Word("nat"),
        "somebody" | "someone" | "anyone" | "anybody" => Word("quén"),
        "everything" => Word("ilqua"),
        "everyone" | "everybody" => Word("ilquen"),
        "am" => Copula,
        _ => return None,
    })
}

/// Irregular closed-class forms whose Wiktionary entries are noisy (e.g. "is" =
/// plural of the letter i): map the surface form to its lemma and features.
pub fn irregular(key: &str) -> Option<(&'static str, u64)> {
    use tkdata::features as f;
    Some(match key {
        "am" => ("be", f::PRES | f::P1 | f::SG),
        "is" => ("be", f::PRES | f::P3 | f::SG),
        "are" => ("be", f::PRES | f::PL),
        "was" => ("be", f::PAST | f::SG),
        "were" => ("be", f::PAST | f::PL),
        "been" => ("be", f::PTCP | f::PAST | f::PERF),
        "being" => ("be", f::PTCP | f::PRES),
        "'s" | "’s" => ("be", f::PRES | f::P3 | f::SG),
        "has" | "hath" => ("have", f::PRES | f::P3 | f::SG),
        "had" => ("have", f::PAST),
        "having" => ("have", f::PTCP | f::PRES),
        "does" | "doth" => ("do", f::PRES | f::P3 | f::SG),
        "did" => ("do", f::PAST),
        "done" => ("do", f::PTCP | f::PAST),
        "doing" => ("do", f::PTCP | f::PRES),
        "could" => ("can", f::PAST),
        "might" => ("may", f::PAST),
        "shall" => ("will", f::PRES),
        "should" => ("would", f::PRES),
        "went" => ("go", f::PAST),
        "gone" => ("go", f::PTCP | f::PAST),
        "goes" => ("go", f::PRES | f::P3 | f::SG),
        "ate" => ("eat", f::PAST),
        "eaten" => ("eat", f::PTCP | f::PAST),
        "saw" => ("see", f::PAST),
        "seen" => ("see", f::PTCP | f::PAST),
        "gave" => ("give", f::PAST),
        "given" => ("give", f::PTCP | f::PAST),
        "took" => ("take", f::PAST),
        "taken" => ("take", f::PTCP | f::PAST),
        "came" => ("come", f::PAST),
        "said" => ("say", f::PAST),
        "made" => ("make", f::PAST),
        "knew" => ("know", f::PAST),
        "known" => ("know", f::PTCP | f::PAST),
        "thought" => ("think", f::PAST),
        "found" => ("find", f::PAST),
        "told" => ("tell", f::PAST),
        "became" => ("become", f::PAST),
        "left" => ("leave", f::PAST),
        "felt" => ("feel", f::PAST),
        "brought" => ("bring", f::PAST),
        "began" => ("begin", f::PAST),
        "begun" => ("begin", f::PTCP | f::PAST),
        "kept" => ("keep", f::PAST),
        "held" => ("hold", f::PAST),
        "wrote" => ("write", f::PAST),
        "written" => ("write", f::PTCP | f::PAST),
        "stood" => ("stand", f::PAST),
        "heard" => ("hear", f::PAST),
        "meant" => ("mean", f::PAST),
        "met" => ("meet", f::PAST),
        "ran" => ("run", f::PAST),
        "paid" => ("pay", f::PAST),
        "sat" => ("sit", f::PAST),
        "spoke" => ("speak", f::PAST),
        "spoken" => ("speak", f::PTCP | f::PAST),
        "lay" => ("lie", f::PAST),
        "led" => ("lead", f::PAST),
        "grew" => ("grow", f::PAST),
        "grown" => ("grow", f::PTCP | f::PAST),
        "lost" => ("lose", f::PAST),
        "fell" => ("fall", f::PAST),
        "fallen" => ("fall", f::PTCP | f::PAST),
        "sent" => ("send", f::PAST),
        "built" => ("build", f::PAST),
        "understood" => ("understand", f::PAST),
        "drew" => ("draw", f::PAST),
        "broke" => ("break", f::PAST),
        "broken" => ("break", f::PTCP | f::PAST),
        "spent" => ("spend", f::PAST),
        "rose" => ("rise", f::PAST),
        "risen" => ("rise", f::PTCP | f::PAST),
        "drove" => ("drive", f::PAST),
        "bought" => ("buy", f::PAST),
        "wore" => ("wear", f::PAST),
        "worn" => ("wear", f::PTCP | f::PAST),
        "chose" => ("choose", f::PAST),
        "chosen" => ("choose", f::PTCP | f::PAST),
        "sang" => ("sing", f::PAST),
        "sung" => ("sing", f::PTCP | f::PAST),
        "flew" => ("fly", f::PAST),
        "flown" => ("fly", f::PTCP | f::PAST),
        "drank" => ("drink", f::PAST),
        "drunk" => ("drink", f::PTCP | f::PAST),
        "fought" => ("fight", f::PAST),
        "forgot" => ("forget", f::PAST),
        "forgotten" => ("forget", f::PTCP | f::PAST),
        "slept" => ("sleep", f::PAST),
        "threw" => ("throw", f::PAST),
        "thrown" => ("throw", f::PTCP | f::PAST),
        "woke" => ("wake", f::PAST),
        "wept" => ("weep", f::PAST),
        "swore" => ("swear", f::PAST),
        "sworn" => ("swear", f::PTCP | f::PAST),
        "shone" => ("shine", f::PAST),
        "bore" => ("bear", f::PAST),
        "born" | "borne" => ("bear", f::PTCP | f::PAST),
        "died" => ("die", f::PAST),
        "dead" => ("dead", 0),
        "sought" => ("seek", f::PAST),
        "taught" => ("teach", f::PAST),
        "caught" => ("catch", f::PAST),
        "won" => ("win", f::PAST),
        "sold" => ("sell", f::PAST),
        "struck" => ("strike", f::PAST),
        "rode" => ("ride", f::PAST),
        "ridden" => ("ride", f::PTCP | f::PAST),
        "hid" => ("hide", f::PAST),
        "hidden" => ("hide", f::PTCP | f::PAST),
        "sworeth" => ("swear", f::PAST),
        "slew" => ("slay", f::PAST),
        "slain" => ("slay", f::PTCP | f::PAST),
        "sank" => ("sink", f::PAST),
        "shook" => ("shake", f::PAST),
        "bound" => ("bind", f::PAST),
        "fed" => ("feed", f::PAST),
        "fled" => ("flee", f::PAST),
        "bled" => ("bleed", f::PAST),
        "sprang" => ("spring", f::PAST),
        "swam" => ("swim", f::PAST),
        "blew" => ("blow", f::PAST),
        "shot" => ("shoot", f::PAST),
        "lit" => ("light", f::PAST),
        "bent" => ("bend", f::PAST),
        "dug" => ("dig", f::PAST),
        "hung" => ("hang", f::PAST),
        "stole" => ("steal", f::PAST),
        "stolen" => ("steal", f::PTCP | f::PAST),
        "froze" => ("freeze", f::PAST),
        "arose" => ("arise", f::PAST),
        "awoke" => ("awake", f::PAST),
        "beheld" => ("behold", f::PAST),
        "besought" => ("beseech", f::PAST),
        "forsook" => ("forsake", f::PAST),
        "forsaken" => ("forsake", f::PTCP | f::PAST),
        "wrought" => ("work", f::PAST),
        "clad" => ("clothe", f::PTCP | f::PAST),
        "don't" | "doesn't" | "didn't" | "cannot" | "can't" | "won't" | "isn't" | "aren't" | "wasn't" | "weren't" | "n't" | "ain't" => ("not", 0),
        "men" => ("man", f::PL),
        "women" => ("woman", f::PL),
        "children" => ("child", f::PL),
        "feet" => ("foot", f::PL),
        "teeth" => ("tooth", f::PL),
        "mice" => ("mouse", f::PL),
        "geese" => ("goose", f::PL),
        "oxen" => ("ox", f::PL),
        "elves" => ("elf", f::PL),
        "dwarves" | "dwarfs" => ("dwarf", f::PL),
        "wolves" => ("wolf", f::PL),
        "leaves" => ("leaf", f::PL),
        "lives" => ("life", f::PL),
        "knives" => ("knife", f::PL),
        "wives" => ("wife", f::PL),
        "thieves" => ("thief", f::PL),
        "halves" => ("half", f::PL),
        "selves" => ("self", f::PL),
        "people" => ("people", 0),
        _ => return None,
    })
}

/// Function-word roles that block multiword-expression merging when they begin the expression.
pub fn is_function_role(r: &Role) -> bool {
    matches!(r, Role::DefArt | Role::IndefArt | Role::Demonstrative { .. } | Role::PossDet(_) | Role::Pronoun { .. } | Role::Copula | Role::Aux(_) | Role::Neg | Role::Conj(_) | Role::Wh(_) | Role::There | Role::Prep(_) | Role::Quant(_) | Role::Number(_))
}

/// Expand common English contractions into their component words.
pub fn expand_contraction(w: &str) -> Option<Vec<&'static str>> {
    let l = w.to_lowercase().replace('’', "'");
    Some(match l.as_str() {
        "don't" => vec!["do", "not"],
        "doesn't" => vec!["does", "not"],
        "didn't" => vec!["did", "not"],
        "can't" | "cannot" => vec!["can", "not"],
        "couldn't" => vec!["could", "not"],
        "won't" => vec!["will", "not"],
        "wouldn't" => vec!["would", "not"],
        "shan't" => vec!["shall", "not"],
        "shouldn't" => vec!["should", "not"],
        "isn't" => vec!["is", "not"],
        "aren't" => vec!["are", "not"],
        "wasn't" => vec!["was", "not"],
        "weren't" => vec!["were", "not"],
        "hasn't" => vec!["has", "not"],
        "haven't" => vec!["have", "not"],
        "hadn't" => vec!["had", "not"],
        "mustn't" => vec!["must", "not"],
        "needn't" => vec!["need", "not"],
        "mightn't" => vec!["might", "not"],
        "ain't" => vec!["is", "not"],
        "i'm" => vec!["i", "am"],
        "i've" => vec!["i", "have"],
        "i'll" => vec!["i", "will"],
        "i'd" => vec!["i", "would"],
        "you're" => vec!["you", "are"],
        "you've" => vec!["you", "have"],
        "you'll" => vec!["you", "will"],
        "you'd" => vec!["you", "would"],
        "he's" => vec!["he", "is"],
        "he'll" => vec!["he", "will"],
        "he'd" => vec!["he", "would"],
        "she's" => vec!["she", "is"],
        "she'll" => vec!["she", "will"],
        "she'd" => vec!["she", "would"],
        "it's" => vec!["it", "is"],
        "it'll" => vec!["it", "will"],
        "we're" => vec!["we", "are"],
        "we've" => vec!["we", "have"],
        "we'll" => vec!["we", "will"],
        "we'd" => vec!["we", "would"],
        "they're" => vec!["they", "are"],
        "they've" => vec!["they", "have"],
        "they'll" => vec!["they", "will"],
        "they'd" => vec!["they", "would"],
        "that's" => vec!["that", "is"],
        "there's" => vec!["there", "is"],
        "here's" => vec!["here", "is"],
        "what's" => vec!["what", "is"],
        "who's" => vec!["who", "is"],
        "where's" => vec!["where", "is"],
        "how's" => vec!["how", "is"],
        "let's" => vec!["let", "us"],
        "y'all" => vec!["you"],
        "o'clock" => vec!["hour"],
        _ => return None,
    })
}

/// A guessed lemma with the features implied by the stripped ending.
pub struct Guess {
    pub lemma: String,
    pub tags: u64,
    /// Coarse part of speech implied by the ending (tkdata pos code, 0 = any).
    pub pos: u8,
}

fn undouble(s: &str) -> Option<String> {
    let cs: Vec<char> = s.chars().collect();
    let n = cs.len();
    if n >= 3 && cs[n - 1] == cs[n - 2] && !"aeiou".contains(cs[n - 1]) {
        Some(cs[..n - 1].iter().collect())
    } else {
        None
    }
}

/// Generate lemma guesses for an English word by regular suffix stripping.
/// The caller must check each guess against the dictionary.
pub fn guess_lemmas(w: &str) -> Vec<Guess> {
    use tkdata::pos;
    let mut out = Vec::new();
    let n = w.len();
    let push = |out: &mut Vec<Guess>, lemma: String, tags: u64, pos: u8| {
        if lemma.len() >= 2 && !out.iter().any(|g: &Guess| g.lemma == lemma && g.tags == tags) {
            out.push(Guess { lemma, tags, pos });
        }
    };
    if let Some(b) = w.strip_suffix("'s").or_else(|| w.strip_suffix("’s")) {
        push(&mut out, b.to_string(), f::GEN, pos::NOUN);
        return out;
    }
    if let Some(b) = w.strip_suffix("s'").or_else(|| w.strip_suffix("s’")) {
        push(&mut out, b.to_string(), f::GEN | f::PL, pos::NOUN);
        return out;
    }
    if n > 4 {
        if let Some(b) = w.strip_suffix("ies") {
            push(&mut out, format!("{}y", b), f::PL, pos::NOUN);
            push(&mut out, format!("{}y", b), f::P3 | f::SG | f::PRES, pos::VERB);
        }
        if let Some(b) = w.strip_suffix("ves") {
            push(&mut out, format!("{}f", b), f::PL, pos::NOUN);
            push(&mut out, format!("{}fe", b), f::PL, pos::NOUN);
        }
        if let Some(b) = w.strip_suffix("es") {
            push(&mut out, b.to_string(), f::PL, pos::NOUN);
            push(&mut out, b.to_string(), f::P3 | f::SG | f::PRES, pos::VERB);
        }
        if let Some(b) = w.strip_suffix("men") {
            push(&mut out, format!("{}man", b), f::PL, pos::NOUN);
        }
    }
    if n > 3 {
        if let Some(b) = w.strip_suffix('s') {
            if !b.ends_with('s') {
                push(&mut out, b.to_string(), f::PL, pos::NOUN);
                push(&mut out, b.to_string(), f::P3 | f::SG | f::PRES, pos::VERB);
            }
        }
    }
    if n > 4 {
        if let Some(b) = w.strip_suffix("ied") {
            push(&mut out, format!("{}y", b), f::PAST, pos::VERB);
            push(&mut out, format!("{}y", b), f::PTCP | f::PASS, pos::VERB);
        }
        if let Some(b) = w.strip_suffix("ed") {
            push(&mut out, b.to_string(), f::PAST, pos::VERB);
            push(&mut out, format!("{}e", b), f::PAST, pos::VERB);
            if let Some(u) = undouble(b) {
                push(&mut out, u, f::PAST, pos::VERB);
            }
            push(&mut out, b.to_string(), f::PTCP | f::PASS, pos::VERB);
            push(&mut out, format!("{}e", b), f::PTCP | f::PASS, pos::VERB);
        }
        if let Some(b) = w.strip_suffix("ing") {
            push(&mut out, b.to_string(), f::PTCP | f::PRES, pos::VERB);
            push(&mut out, format!("{}e", b), f::PTCP | f::PRES, pos::VERB);
            if let Some(u) = undouble(b) {
                push(&mut out, u, f::PTCP | f::PRES, pos::VERB);
            }
        }
        if let Some(b) = w.strip_suffix("ily") {
            push(&mut out, format!("{}y", b), 0, pos::ADV);
        }
        if let Some(b) = w.strip_suffix("ly") {
            push(&mut out, b.to_string(), 0, pos::ADV);
            if b.ends_with("al") {
                push(&mut out, b[..b.len() - 2].to_string(), 0, pos::ADV);
            }
        }
        if let Some(b) = w.strip_suffix("ier") {
            push(&mut out, format!("{}y", b), f::CMP, pos::ADJ);
        }
        if let Some(b) = w.strip_suffix("iest") {
            push(&mut out, format!("{}y", b), f::SUP, pos::ADJ);
        }
        if let Some(b) = w.strip_suffix("er") {
            push(&mut out, b.to_string(), f::CMP, pos::ADJ);
            push(&mut out, format!("{}e", b), f::CMP, pos::ADJ);
            if let Some(u) = undouble(b) {
                push(&mut out, u, f::CMP, pos::ADJ);
            }
        }
        if let Some(b) = w.strip_suffix("est") {
            push(&mut out, b.to_string(), f::SUP, pos::ADJ);
            push(&mut out, format!("{}e", b), f::SUP, pos::ADJ);
            if let Some(u) = undouble(b) {
                push(&mut out, u, f::SUP, pos::ADJ);
            }
        }
    }
    if n > 3 {
        if let Some(b) = w.strip_prefix("un") {
            push(&mut out, b.to_string(), f::NEG, 0);
        }
    }
    out
}

/// English lemmas without an Elvish entry that a near-synonym covers well
/// (Tolkien's own vocabulary first: "hobbit" is Quenya *Perian* "Halfling").
pub fn synonym(key: &str) -> Option<&'static str> {
    Some(match key {
        "hobbit" => "halfling",
        "goblin" => "orc",
        "wizard" | "sorcerer" | "magician" => "wizard",
        "warg" => "wolf",
        "ent" => "tree",
        "automobile" | "car" | "vehicle" => "wagon",
        "computer" => "machine",
        "airplane" | "aeroplane" | "aircraft" => "ship",
        "television" | "screen" => "mirror",
        "telephone" | "phone" => "voice",
        "book" => "book",
        "money" | "coin" => "gold",
        "internet" | "network" => "web",
        "email" | "letter" => "message",
        "bicycle" => "wheel",
        "train" => "wagon",
        "gun" | "rifle" | "pistol" => "weapon",
        "bomb" => "fire",
        "president" | "prime minister" | "chancellor" | "governor" | "mayor" | "emperor" => "ruler",
        "senator" | "minister" | "politician" | "official" => "lord",
        "soldier" | "policeman" | "officer" => "warrior",
        "doctor" | "physician" | "nurse" => "healer",
        "teacher" | "professor" => "master",
        "student" | "pupil" => "learner",
        "scientist" | "engineer" | "programmer" | "developer" => "craftsman",
        "company" | "corporation" | "business" | "firm" => "guild",
        "office" | "factory" | "shop" | "store" => "house",
        "city" | "metropolis" => "city",
        "village" | "town" => "town",
        "university" | "school" | "college" | "academy" => "school",
        "hospital" => "house of healing",
        "church" | "temple" | "cathedral" => "temple",
        "restaurant" | "cafe" | "café" | "bar" | "pub" => "inn",
        "hotel" => "inn",
        "software" | "program" | "application" | "app" | "code" => "spell",
        "database" | "file" | "document" | "data" => "record",
        "user" => "person",
        "server" | "system" => "device",
        "website" | "page" => "leaf",
        "video" | "movie" | "film" => "vision",
        "music" => "music",
        "photo" | "photograph" | "picture" | "image" => "image",
        "weekend" => "rest",
        "minute" | "second" => "moment",
        "hour" => "hour",
        "week" => "week",
        "month" => "month",
        "century" => "age",
        "percent" | "percentage" => "part",
        "dollar" | "euro" | "pound" | "yen" => "coin",
        "problem" | "issue" | "bug" => "trouble",
        "project" => "work",
        "team" => "company",
        "customer" | "client" => "guest",
        "market" => "market",
        "price" | "cost" => "worth",
        "job" | "career" => "work",
        "news" => "tidings",
        "information" => "knowledge",
        "idea" | "concept" => "thought",
        "example" | "instance" => "sample",
        "result" | "outcome" => "fruit",
        "function" | "feature" => "power",
        "version" | "edition" => "form",
        "test" | "exam" => "trial",
        "error" | "mistake" | "fault" => "error",
        "level" | "degree" | "grade" => "step",
        "type" | "kind" | "sort" => "kind",
        "list" => "list",
        "option" | "choice" => "choice",
        "default" => "custom",
        "note" | "remark" | "comment" => "word",
        "figure" | "number" => "number",
        "table" => "table",
        "text" => "writing",
        "sentence" | "phrase" | "clause" => "saying",
        "language" | "tongue" => "language",
        "dictionary" | "lexicon" | "vocabulary" => "word-hoard",
        _ => return None,
    })
}

/// English keys that are dictionary artefacts rather than translations
/// ("surname", "given name", topic labels...).
pub fn is_artefact(key: &str) -> bool {
    matches!(
        key,
        "surname"
            | "given name"
            | "male given name"
            | "female given name"
            | "placeholder"
            | "singular"
            | "plural"
            | "genitive"
            | "dative"
            | "accusative"
            | "nominative"
            | "chess"
            | "card game"
            | "card games"
            | "movies"
            | "etc"
            | "etc."
            | "astronomy"
            | "gymnastics"
            | "botany"
            | "zoology"
            | "anatomy"
            | "music"
            | "mathematics"
            | "grammar"
            | "linguistics"
            | "slang"
            | "vulgar"
            | "colloquial"
            | "informal"
            | "formal"
            | "obsolete"
            | "archaic"
            | "figuratively"
            | "transitive"
            | "intransitive"
            | "reflexive"
            | "diminutive"
            | "augmentative"
            | "abbreviation"
            | "initialism"
            | "acronym"
            | "alternative form"
            | "alternative spelling"
            | "misspelling"
            | "letter"
            | "symbol"
            | "sports"
            | "computing"
            | "medicine"
            | "law"
            | "military"
            | "nautical"
            | "religion"
            | "politics"
            | "physics"
            | "chemistry"
            | "biology"
            | "geography"
            | "history"
            | "literature"
            | "used"
            | "uncountable"
            | "countable"
    )
}

/// English lemmas whose most natural rendering is fixed (overrides dictionary
/// scoring for very common words where Eldamo's ranking is misleading).
pub fn preferred(key: &str) -> Option<&'static str> {
    Some(match key {
        "man" => "nér",
        "men" => "neri",
        "woman" => "nís",
        "people" => "lië",
        "person" => "quén",
        "god" => "Eru",
        "lord" => "heru",
        "lady" => "heri",
        "world" => "ambar",
        "earth" => "cemen",
        "sun" => "Anar",
        "moon" => "Isil",
        "day" => "aurë",
        "night" => "lómë",
        "year" => "loa",
        "time" => "lú",
        "way" => "tië",
        "thing" => "nat",
        "word" => "quetta",
        "name" => "essë",
        "house" => "coa",
        "home" => "már",
        "city" => "osto",
        "land" => "nórë",
        "water" => "nén",
        "fire" => "nár",
        "light" => "cala",
        "life" => "cuilë",
        "death" => "qualmë",
        "heart" => "hón",
        "hand" => "má",
        "eye" => "hen",
        "friend" => "meldo",
        "enemy" => "cotto",
        "war" => "ohta",
        "peace" => "sérë",
        "song" => "lírë",
        "book" => "parma",
        "power" => "túrë",
        "hope" => "estel",
        "love" => "melmë",
        "great" => "alta",
        "big" | "large" => "alta",
        "small" => "pitya",
        "good" => "mára",
        "bad" | "evil" => "ulca",
        "new" => "vinya",
        "old" => "yára",
        "long" => "anda",
        "high" | "tall" => "halla",
        "white" => "ninquë",
        "black" => "morë",
        "red" => "carnë",
        "green" => "laica",
        "blue" => "luinë",
        "gold" => "malta",
        "golden" => "laurëa",
        "beautiful" => "vanima",
        "dark" => "morna",
        "holy" => "airë",
        "true" => "anwa",
        "free" => "léra",
        "last" => "métima",
        "go" => "lelya-",
        "come" => "tul-",
        "see" => "cen-",
        "know" => "ista-",
        "say" => "quet-",
        "speak" | "talk" => "quet-",
        "make" => "car-",
        "do" => "car-",
        "give" => "anta-",
        "take" => "mapa-",
        "find" => "hir-",
        "want" | "wish" => "mer-",
        "think" => "sana-",
        "live" => "cuita-",
        "die" => "fir-",
        "sing" => "lir-",
        "walk" => "vanta-",
        "run" => "nor-",
        "hear" => "hlar-",
        "look" | "watch" => "tir-",
        "call" => "yal-",
        "eat" => "mat-",
        "drink" => "suc-",
        "sleep" => "lor-",
        "write" => "tec-",
        "read" => "henta-",
        "work" => "mól-",
        _ => return None,
    })
}

/// Roughly the 500 most frequent English words, most frequent first. Used only
/// to break ties between equally weighted glosses of a foreign word.
const FREQUENT: &[&str] = &[
    "the", "be", "to", "of", "and", "a", "in", "that", "have", "i", "it", "for", "not", "on", "with", "he", "as", "you", "do", "at", "this", "but", "his", "by", "from", "they", "we", "say", "her", "she", "or", "an", "will", "my", "one", "all", "would", "there", "their", "what", "so", "up", "out", "if", "about", "who", "get", "which", "go", "me", "when", "make", "can", "like", "time", "no", "just", "him", "know", "take", "people", "into", "year", "your", "good", "some", "could", "them", "see", "other", "than", "then", "now", "give", "take", "make", "know", "see", "come", "look", "only", "its", "over", "think", "also", "back", "after", "use", "two", "how", "our", "work", "first", "well", "way", "even", "new", "want", "because", "any", "these", "give", "day", "most", "us", "is", "are", "was", "were", "been", "has", "had", "did", "very", "great", "man", "woman", "world", "life", "hand", "part", "child", "eye", "place", "week", "case", "point", "government", "company", "number", "group", "problem", "fact", "long", "little", "own", "old", "right", "big", "high", "different", "small", "large", "next", "early", "young", "important", "few", "public", "bad", "same", "able", "find", "tell", "ask", "seem", "feel", "try", "leave", "call", "keep", "let", "begin", "help", "show", "hear", "play", "run", "move", "live", "believe", "bring", "happen", "write", "sit", "stand", "lose", "pay", "meet", "include", "continue", "set", "learn", "change", "lead", "understand", "watch", "follow", "stop", "create", "speak", "read", "spend", "grow", "open", "walk", "win", "teach", "offer", "remember", "consider", "appear", "buy", "wait", "serve", "die", "send", "build", "stay", "fall", "cut", "reach", "kill", "raise", "pass", "sell", "decide", "return", "explain", "hope", "develop", "carry", "break", "receive", "agree", "support", "hit", "produce", "eat", "cover", "catch", "draw", "choose", "cause", "hold", "turn", "put", "mean", "become", "need", "may", "should", "must", "might", "shall", "here", "where", "why", "again", "still", "never", "always", "often", "while", "before", "under", "between", "through", "during", "without", "against", "among", "toward", "upon", "within", "along", "across", "behind", "beyond", "near", "around", "above", "below", "off", "down", "away", "together", "once", "much", "many", "more", "less", "every", "each", "both", "another", "such", "last", "own", "sure", "true", "free", "full", "whole", "black", "white", "red", "green", "blue", "dark", "light", "cold", "hot", "warm", "deep", "wide", "strong", "hard", "soft", "clear", "dead", "alive", "happy", "sad", "beautiful", "fair", "wise", "holy", "evil", "house", "home", "land", "sea", "water", "fire", "earth", "sky", "sun", "moon", "star", "night", "morning", "evening", "wind", "tree", "stone", "mountain", "river", "road", "way", "door", "room", "city", "country", "war", "peace", "king", "queen", "lord", "lady", "father", "mother", "son", "daughter", "brother", "sister", "friend", "enemy", "god", "death", "love", "heart", "mind", "word", "name", "song", "voice", "head", "face", "foot", "body", "blood", "power", "gold", "silver", "sword", "ship", "horse", "bird", "dog", "cat", "book", "letter", "end", "beginning", "thing", "nothing", "something", "anything", "everything", "someone", "everyone", "nobody", "yes", "please", "thank", "hello", "goodbye", "very", "quite", "rather", "too", "enough", "almost", "already", "yet", "soon", "late", "ago", "today", "tomorrow", "yesterday", "tonight", "far", "fast", "slow", "quick", "sudden", "silent", "loud", "sweet", "bitter", "sharp", "bright", "grey", "gray", "brown", "yellow", "go", "come", "give", "take", "see", "hear", "know", "think", "say", "speak", "tell", "sing", "shine", "sleep", "dream", "wake", "fly", "fight", "flee", "hide", "seek", "find", "lose", "love", "hate", "fear", "hope", "wish", "want", "desire", "wonder", "remember", "forget", "forgive", "bless", "curse", "swear", "promise", "guard", "protect", "rule", "reign", "own", "possess", "dwell", "abide", "remain", "depart", "arrive", "journey", "travel", "ride", "sail", "wander", "return", "answer", "question", "master", "servant", "slave", "man", "elf", "dwarf", "orc", "dragon", "wizard", "ring", "tower", "hall", "gate", "wall", "bridge", "forest", "wood", "hill", "valley", "field", "garden", "flower", "leaf", "root", "branch", "grass", "snow", "rain", "cloud", "storm", "shadow", "darkness", "light", "dawn", "dusk", "spring", "summer", "autumn", "winter", "age", "hour", "moment", "path", "shore", "wave", "island", "north", "south", "east", "west", "left", "right", "middle", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "hundred", "thousand", "first", "second", "third",
];

pub fn freq_rank(key: &str) -> Option<usize> {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static MAP: OnceLock<HashMap<&'static str, usize>> = OnceLock::new();
    let m = MAP.get_or_init(|| {
        let mut m = HashMap::with_capacity(FREQUENT.len());
        for (i, w) in FREQUENT.iter().enumerate() {
            m.entry(*w).or_insert(i);
        }
        m
    });
    m.get(key).copied()
}

/// Bonus (0–4) for frequent English glosses; ties between equally weighted senses.
pub fn freq_bonus(key: &str) -> f32 {
    match freq_rank(key) {
        Some(r) => 4.0 * (1.0 - r as f32 / FREQUENT.len() as f32),
        None => 0.0,
    }
}
