//! Particles and postpositions of Japanese, Chinese and Korean, and the mapping of their
//! grammatical functions onto Quenya cases.

use crate::quenya::Case;
use tkdata::features as f;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PKind {
    /// Case marker on the preceding phrase (closes the phrase).
    Case,
    /// Genitive/attributive marker: the preceding noun modifies the following one.
    Gen,
    /// Sentence-final question particle.
    Question,
    /// No Quenya counterpart.
    Drop,
    /// Rendered as an English function word.
    Word(&'static str),
    /// "and" between nominals, otherwise "with".
    AndWith,
    /// Chinese 把: the following phrase is the direct object.
    ObjNext,
    /// Perfective/past marker on the preceding verb.
    Past,
    /// Plural marker on the preceding nominal.
    Plural,
    /// Adverbialiser after an adjective (Chinese 地).
    AdvMarker,
    /// Sentence-final copula (Japanese だ/です) with its tense/politeness features.
    Copula(u64),
}

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub text: &'static str,
    pub kind: PKind,
    /// Case bits forced on the phrase head (several bits = resolved by context later).
    pub case: u64,
    /// Only recognised at the end of a run (sentence-final particles).
    pub final_only: bool,
    /// Segmentation cost (lower than a lexical reading of the same characters).
    pub cost: f32,
}

const fn p(text: &'static str, kind: PKind, case: u64) -> Particle {
    Particle { text, kind, case, final_only: false, cost: 0.4 }
}

const fn fin(text: &'static str, kind: PKind) -> Particle {
    Particle { text, kind, case: 0, final_only: true, cost: 0.3 }
}

pub const JA: &[Particle] = &[
    // compound particles first (longest match is not automatic: costs decide)
    p("には", PKind::Case, f::DAT | f::LOC | f::ALL),
    p("では", PKind::Case, f::LOC | f::INS),
    p("へは", PKind::Case, f::ALL),
    p("からは", PKind::Case, f::ABL),
    p("までに", PKind::Case, f::ALL),
    p("とは", PKind::Drop, 0),
    p("にも", PKind::Case, f::DAT | f::LOC | f::ALL),
    p("でも", PKind::Case, f::LOC | f::INS),
    p("は", PKind::Case, f::NOM),
    p("が", PKind::Case, f::NOM),
    p("を", PKind::Case, f::ACC),
    p("に", PKind::Case, f::DAT | f::LOC | f::ALL),
    p("へ", PKind::Case, f::ALL),
    p("で", PKind::Case, f::LOC | f::INS),
    p("の", PKind::Gen, f::GEN),
    p("と", PKind::AndWith, 0),
    p("や", PKind::Word("and"), 0),
    p("も", PKind::Word("also"), f::NOM),
    p("から", PKind::Case, f::ABL),
    p("まで", PKind::Case, f::ALL),
    p("より", PKind::Word("than"), 0),
    p("だけ", PKind::Word("only"), 0),
    p("しか", PKind::Word("only"), 0),
    p("こそ", PKind::Case, f::NOM),
    p("たち", PKind::Plural, f::PL),
    fin("でした", PKind::Copula(f::PAST | f::FORMAL)),
    fin("だった", PKind::Copula(f::PAST)),
    fin("であった", PKind::Copula(f::PAST)),
    fin("です", PKind::Copula(f::PRES | f::FORMAL)),
    fin("である", PKind::Copula(f::PRES)),
    fin("だ", PKind::Copula(f::PRES)),
    fin("ですか", PKind::Question),
    fin("でしたか", PKind::Question),
    fin("か", PKind::Question),
    fin("かな", PKind::Question),
    fin("の", PKind::Question),
    fin("ね", PKind::Drop),
    fin("よ", PKind::Drop),
    fin("わ", PKind::Drop),
    fin("ぞ", PKind::Drop),
    fin("ぜ", PKind::Drop),
    fin("さ", PKind::Drop),
    fin("な", PKind::Drop),
    fin("よね", PKind::Drop),
    fin("のだ", PKind::Drop),
    fin("んだ", PKind::Drop),
    fin("のです", PKind::Drop),
    fin("んです", PKind::Drop),
];

pub const ZH: &[Particle] = &[
    p("和", PKind::AndWith, 0),
    p("與", PKind::AndWith, 0),
    p("与", PKind::AndWith, 0),
    p("跟", PKind::AndWith, 0),
    p("及", PKind::AndWith, 0),
    p("了", PKind::Past, f::PAST),
    p("過", PKind::Past, f::PAST),
    p("过", PKind::Past, f::PAST),
    p("著", PKind::Drop, 0),
    p("着", PKind::Drop, 0),
    p("的", PKind::Gen, f::GEN),
    p("地", PKind::AdvMarker, 0),
    p("把", PKind::ObjNext, f::ACC),
    p("將", PKind::ObjNext, f::ACC),
    p("将", PKind::ObjNext, f::ACC),
    p("們", PKind::Plural, f::PL),
    p("们", PKind::Plural, f::PL),
    fin("嗎", PKind::Question),
    fin("吗", PKind::Question),
    fin("呢", PKind::Drop),
    fin("吧", PKind::Drop),
    fin("啊", PKind::Drop),
    fin("呀", PKind::Drop),
    fin("嘛", PKind::Drop),
    fin("哦", PKind::Drop),
    fin("啦", PKind::Drop),
    fin("了嗎", PKind::Question),
    fin("了吗", PKind::Question),
];

/// Korean postpositions (the text is space-separated; these are stripped from word ends).
pub const KO: &[Particle] = &[
    p("에게서", PKind::Case, f::ABL),
    p("으로부터", PKind::Case, f::ABL),
    p("로부터", PKind::Case, f::ABL),
    p("에서는", PKind::Case, f::LOC | f::ABL),
    p("에서", PKind::Case, f::LOC | f::ABL),
    p("에게는", PKind::Case, f::DAT),
    p("에게", PKind::Case, f::DAT),
    p("께서", PKind::Case, f::NOM),
    p("께", PKind::Case, f::DAT),
    p("으로는", PKind::Case, f::INS | f::ALL),
    p("으로", PKind::Case, f::INS | f::ALL),
    p("로는", PKind::Case, f::INS | f::ALL),
    p("로", PKind::Case, f::INS | f::ALL),
    p("부터", PKind::Case, f::ABL),
    p("까지", PKind::Case, f::ALL),
    p("보다", PKind::Word("than"), 0),
    p("처럼", PKind::Word("like"), 0),
    p("같이", PKind::Word("like"), 0),
    p("하고", PKind::AndWith, 0),
    p("이랑", PKind::AndWith, 0),
    p("랑", PKind::AndWith, 0),
    p("과", PKind::AndWith, 0),
    p("와", PKind::AndWith, 0),
    p("에는", PKind::Case, f::LOC | f::ALL | f::DAT),
    p("에", PKind::Case, f::LOC | f::ALL | f::DAT),
    p("은", PKind::Case, f::NOM),
    p("는", PKind::Case, f::NOM),
    p("이", PKind::Case, f::NOM),
    p("가", PKind::Case, f::NOM),
    p("을", PKind::Case, f::ACC),
    p("를", PKind::Case, f::ACC),
    p("의", PKind::Gen, f::GEN),
    p("도", PKind::Word("also"), f::NOM),
    p("만", PKind::Word("only"), 0),
    p("들", PKind::Plural, f::PL),
];

/// Korean copula endings attached directly to a noun ("왕이다" = "is a king").
pub const KO_COPULA: &[(&str, u64)] = &[
    ("이었습니다", f::PAST | f::FORMAL),
    ("이었다", f::PAST),
    ("였습니다", f::PAST | f::FORMAL),
    ("였다", f::PAST),
    ("입니다", f::PRES | f::FORMAL),
    ("입니까", f::PRES | f::FORMAL),
    ("이에요", f::PRES | f::FORMAL),
    ("예요", f::PRES | f::FORMAL),
    ("이다", f::PRES),
    ("이야", f::PRES),
];

pub fn particles(lang: &str) -> &'static [Particle] {
    match lang {
        "ja" => JA,
        "zh" | "yue" | "lzh" | "nan" | "hak" => ZH,
        "ko" => KO,
        _ => &[],
    }
}

/// Verbs of motion: a "to/at" particle marks the goal (allative).
const MOTION_VERBS: &[&str] = &[
    "go", "come", "return", "arrive", "travel", "walk", "run", "fly", "ride", "sail", "climb", "enter", "fall", "flow",
    "move", "descend", "ascend", "lead", "send", "bring", "carry", "throw", "reach", "depart", "leave", "journey",
    "wander", "hurry", "escape", "flee", "follow", "gather", "put", "place", "set", "sit down", "lie down", "step",
];

/// Verbs of location/existence: a "to/at" particle marks the place (locative).
const LOCATION_VERBS: &[&str] = &[
    "be", "exist", "live", "dwell", "stay", "remain", "stand", "sit", "lie", "wait", "sleep", "rest", "abide",
    "appear", "happen", "occur", "work", "grow", "die", "hide", "burn", "shine", "sing", "dance", "fight", "meet",
    "hold", "keep", "there is", "there are",
];

/// Verbs of giving/telling: a "to" particle marks the recipient (dative).
const RECIPIENT_VERBS: &[&str] = &[
    "give", "hand", "grant", "bestow", "offer", "present", "tell", "say", "speak", "show", "teach", "send", "write",
    "lend", "pay", "sell", "bring", "return", "promise", "answer", "explain", "read", "sing", "pass", "deliver",
    "entrust", "leave", "ask", "pray", "swear", "owe", "throw", "supply", "provide", "afford", "impart", "award",
    "get", "obtain", "receive",
];

/// Nouns that denote places or times: "in/at" rather than "with/by".
const PLACE_NOUNS: &[&str] = &[
    "place", "city", "town", "village", "house", "home", "hall", "room", "land", "country", "kingdom", "realm",
    "world", "forest", "wood", "woods", "mountain", "hill", "valley", "river", "sea", "ocean", "lake", "shore",
    "island", "field", "garden", "road", "street", "way", "path", "market", "school", "castle", "tower", "gate",
    "door", "sky", "heaven", "heavens", "earth", "ground", "north", "south", "east", "west", "middle", "inside",
    "outside", "top", "bottom", "edge", "end", "beginning", "court", "palace", "temple", "church", "library",
    "prison", "battle", "war", "dark", "darkness", "light", "night", "morning", "evening", "day", "year", "time",
    "age", "hour", "spring", "summer", "autumn", "winter", "bridge", "harbour", "harbor", "cave", "mine", "well",
    "throne", "seat", "bed", "table", "chair", "floor", "wall", "roof", "window", "corner", "side", "front", "back",
    "capital", "province", "region", "district", "coast", "desert", "plain", "meadow", "swamp", "marsh", "grave",
    "tomb", "camp", "fortress", "fort", "stronghold", "cell", "chamber", "kitchen", "office", "station", "shop",
    "store", "hospital", "university", "college", "company", "party", "meeting", "war", "west", "shadow", "sun",
    "moon", "star", "stars", "water", "fire", "air", "wind", "rain", "snow", "sea", "tokyo", "japan", "china",
    "korea", "america", "england", "france", "germany", "europe", "asia", "africa", "london", "paris", "beijing",
    "seoul", "kyoto", "osaka", "shanghai", "taiwan", "vietnam", "india", "russia",
];

pub fn is_place_noun(key: &str) -> bool {
    PLACE_NOUNS.contains(&key) || key.ends_with("land") || key.ends_with("town") || key.ends_with("burg") || key.ends_with("ville")
}

/// Ends with a place-denoting character (Japanese/Chinese location nouns).
pub fn is_place_text(text: &str) -> bool {
    text.chars().last().map(|c| "所場国國里京市城山川海家屋室園庭林森島岛都県省村町区区州港湾谷野原駅站路街道門门院寺社校店館舘堂殿宮宫界間中上下内外前後边邊側側方地天空国".contains(c)).unwrap_or(false)
}

/// Choose the Quenya case for a phrase whose particle allows several readings.
pub fn resolve_case(forced: u64, verb_key: &str, noun_key: &str, noun_text: &str) -> Option<Case> {
    let cases = forced & f::CASES;
    if cases == 0 {
        return None;
    }
    let place = is_place_noun(noun_key) || is_place_text(noun_text);
    let motion = MOTION_VERBS.contains(&verb_key);
    let location = LOCATION_VERBS.contains(&verb_key);
    let recipient = RECIPIENT_VERBS.contains(&verb_key);
    let single = |bits: u64| cases == bits;
    if single(f::NOM) || single(f::ACC) || single(f::NOM | f::ACC) {
        return None;
    }
    if single(f::GEN) {
        return Some(Case::Gen);
    }
    if single(f::DAT) {
        return Some(Case::Dat);
    }
    if single(f::ABL) {
        return Some(Case::Abl);
    }
    if single(f::ALL) {
        return Some(Case::All);
    }
    if single(f::LOC) {
        return Some(Case::Loc);
    }
    if single(f::INS) {
        return Some(Case::Ins);
    }
    if cases == (f::DAT | f::LOC | f::ALL) || cases == (f::LOC | f::ALL | f::DAT) {
        // Japanese に / Korean 에
        if motion {
            return Some(Case::All);
        }
        if recipient && !place {
            return Some(Case::Dat);
        }
        if location || place {
            return Some(Case::Loc);
        }
        return Some(if verb_key.is_empty() { Case::Loc } else { Case::Dat });
    }
    if cases == (f::LOC | f::INS) {
        // Japanese で
        return Some(if place { Case::Loc } else { Case::Ins });
    }
    if cases == (f::LOC | f::ABL) {
        // Korean 에서
        return Some(if motion { Case::Abl } else { Case::Loc });
    }
    if cases == (f::INS | f::ALL) {
        // Korean (으)로
        return Some(if motion || place { Case::All } else { Case::Ins });
    }
    crate::translate::case_of_tags(cases)
}

/// Chinese measure words (dropped between a numeral and its noun).
pub fn is_zh_classifier(c: char) -> bool {
    "個个隻只條条張张件位名頭头匹杯瓶輛辆座間间家棵顆颗塊块篮篇幅支枝根雙双對对群種种次回遍場场頓顿封套台部門门所節节首幢層层級级句段頁页步口包盒桶碗盤盘滴片粒串束堆批打副身面道股陣阵番本把冊册卷筆笔架棟栋艘輪轮排列隊队幫帮夥伙滿满盞盏盆袋箱籠笼粒".contains(c)
}

/// Localizers that follow a noun (Chinese 森林里 "in the forest", Japanese 木の下 "under the
/// tree"), with the English preposition they stand for.
pub fn localizer_prep(piece: &str) -> Option<&'static str> {
    Some(match piece {
        "里" | "裡" | "裏" | "中" | "內" | "内" | "里面" | "裡面" | "裏面" | "中間" | "中间" | "之中" | "之內" | "之内" | "の中" => "in",
        "上" | "上面" | "上边" | "上邊" | "之上" => "on",
        "下" | "下面" | "下边" | "下邊" | "之下" | "底下" => "under",
        "前" | "前面" | "前边" | "前邊" | "之前" => "before",
        "後" | "后" | "後面" | "后面" | "后边" | "後邊" | "之後" | "之后" => "behind",
        "外" | "外面" | "外边" | "外邊" | "之外" => "outside",
        "旁" | "旁边" | "旁邊" | "横" | "隣" | "側" | "そば" | "となり" => "beside",
        "間" | "间" | "之間" | "之间" | "あいだ" => "between",
        "近く" | "附近" => "near",
        "向こう" | "対面" => "beyond",
        _ => return None,
    })
}

/// Multi-character pieces that start with the copula swallow it ("是我" = "it's me"):
/// only a few genuine compounds are allowed.
pub fn zh_no_compound(piece: &str) -> bool {
    piece.starts_with('是') && !matches!(piece, "是否" | "是的" | "是非" | "是以" | "是故")
}

/// Existential verbs: 猫がいる / 本がある / 왕이 있다 = "there is …" (ëa).
pub fn is_existential(lang: &str, text: &str) -> bool {
    match lang {
        "ja" => {
            let t = text.trim_end_matches('。');
            matches!(
                t,
                "いる" | "います" | "いた" | "いました" | "いない" | "いません" | "いなかった" | "いませんでした" | "おる" | "おります" | "ある" | "あります" | "あった" | "ありました" | "ない" | "ありません" | "なかった" | "ありませんでした" | "ございます" | "存在する" | "存在します"
            )
        }
        "ko" => text.starts_with("있") || text.starts_with("없") || text.starts_with("계시"),
        _ => false,
    }
}
