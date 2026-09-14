//! Canonical grammatical feature tags (bit positions must match tools/kcommon.py FEATURES).

pub const NAMES: [&str; 56] = [
    "sg", "pl", "du", "nom", "gen", "dat", "acc", "ins", "loc", "abl", "all", "voc", "poss", "part", "ess",
    "ine", "ela", "ade", "com", "tra", "pres", "past", "fut", "perf", "impf", "aor", "ind", "subj", "cond",
    "imp", "inf", "ptcp", "ger", "opt", "act", "pass", "p1", "p2", "p3", "masc", "fem", "neut", "cmp", "sup",
    "neg", "def", "indef", "dim", "aug", "formal", "informal", "formof", "alt", "archaic", "refl", "obj",
];

macro_rules! feat {
    ($name:ident, $idx:expr) => {
        pub const $name: u64 = 1u64 << $idx;
    };
}

feat!(SG, 0);
feat!(PL, 1);
feat!(DU, 2);
feat!(NOM, 3);
feat!(GEN, 4);
feat!(DAT, 5);
feat!(ACC, 6);
feat!(INS, 7);
feat!(LOC, 8);
feat!(ABL, 9);
feat!(ALL, 10);
feat!(VOC, 11);
feat!(POSS, 12);
feat!(PART, 13);
feat!(ESS, 14);
feat!(INE, 15);
feat!(ELA, 16);
feat!(ADE, 17);
feat!(COM, 18);
feat!(TRA, 19);
feat!(PRES, 20);
feat!(PAST, 21);
feat!(FUT, 22);
feat!(PERF, 23);
feat!(IMPF, 24);
feat!(AOR, 25);
feat!(IND, 26);
feat!(SUBJ, 27);
feat!(COND, 28);
feat!(IMP, 29);
feat!(INF, 30);
feat!(PTCP, 31);
feat!(GER, 32);
feat!(OPT, 33);
feat!(ACT, 34);
feat!(PASS, 35);
feat!(P1, 36);
feat!(P2, 37);
feat!(P3, 38);
feat!(MASC, 39);
feat!(FEM, 40);
feat!(NEUT, 41);
feat!(CMP, 42);
feat!(SUP, 43);
feat!(NEG, 44);
feat!(DEF, 45);
feat!(INDEF, 46);
feat!(DIM, 47);
feat!(AUG, 48);
feat!(FORMAL, 49);
feat!(INFORMAL, 50);
feat!(FORMOF, 51);
feat!(ALT, 52);
feat!(ARCHAIC, 53);
feat!(REFL, 54);
feat!(OBJ, 55);
// part of speech of the source page an alias link was taken from (bits 56-59)
feat!(POS_N, 56);
feat!(POS_V, 57);
feat!(POS_ADJ, 58);
feat!(POS_ADV, 59);
pub const POS_ANY: u64 = POS_N | POS_V | POS_ADJ | POS_ADV;

pub const NUMBER: u64 = SG | PL | DU;
pub const CASES: u64 = NOM | GEN | DAT | ACC | INS | LOC | ABL | ALL | VOC | PART | ESS | INE | ELA | ADE | COM | TRA;
pub const TENSES: u64 = PRES | PAST | FUT | PERF | IMPF | AOR;
pub const MOODS: u64 = IND | SUBJ | COND | IMP | INF | PTCP | GER | OPT;
pub const PERSONS: u64 = P1 | P2 | P3;
pub const GENDERS: u64 = MASC | FEM | NEUT;
pub const DEGREES: u64 = CMP | SUP;

pub fn bit(name: &str) -> Option<u64> {
    NAMES.iter().position(|n| *n == name).map(|i| 1u64 << i)
}

/// Parse a comma separated list of tag names into a bit set (unknown names ignored).
pub fn parse(tags: &str) -> u64 {
    let mut bits = 0u64;
    for t in tags.split(',') {
        let t = t.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(b) = bit(t) {
            bits |= b;
        } else {
            bits |= match t {
                "n" => POS_N,
                "vb" => POS_V,
                "adj" => POS_ADJ,
                "adv" => POS_ADV,
                _ => 0,
            };
        }
    }
    bits
}

pub fn format(bits: u64) -> String {
    let mut out = Vec::new();
    for (i, n) in NAMES.iter().enumerate() {
        if bits & (1u64 << i) != 0 {
            out.push(*n);
        }
    }
    out.join(",")
}

/// Human-readable list of the feature names set in `t`.
pub fn describe(t: u64) -> String {
    let mut out = Vec::new();
    for (i, n) in NAMES.iter().enumerate() {
        if t & (1u64 << i) != 0 {
            out.push(*n);
        }
    }
    if out.is_empty() {
        "-".to_string()
    } else {
        out.join(",")
    }
}
