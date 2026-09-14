//! Quenya → Tengwar transcription (classical Quenya mode) using the ConScript
//! Unicode Registry (CSUR) private-use code points U+E000–U+E07F, which the common
//! Tengwar fonts (Tengwar Telcontar, FreeMonoTengwar, Tengwar Formal CSUR…) support.
//!
//! Rules of the Quenya mode as used in Namárië and the King's Letter:
//! * a tehta (vowel sign) sits on the tengwa of the consonant that *precedes* it;
//!   a vowel without such a consonant, and every long vowel, rides a carrier
//!   (short E02E, long E02C);
//! * ai/oi/ui are yanta + tehta, au/eu/iu are úrë + tehta;
//! * r before a vowel is rómen, otherwise órë; s bearing a tehta is silmë nuquerna;
//! * the pre-nasalised and doubled stops (nd, mb, ng, ngw, nt, mp, nc, nqu) and the
//!   clusters qu, hw, rd, ld, ss, th have their own tengwar; ty/ly/ny/ry/hy take the
//!   two dots (E043) below the base letter; other doubled consonants take the doubler bar.

const TINCO: char = '\u{E000}';
const PARMA: char = '\u{E001}';
const CALMA: char = '\u{E002}';
const QUESSE: char = '\u{E003}';
const ANDO: char = '\u{E004}';
const UMBAR: char = '\u{E005}';
const ANGA: char = '\u{E006}';
const UNGWE: char = '\u{E007}';
const THULE: char = '\u{E008}';
const FORMEN: char = '\u{E009}';
const AHA: char = '\u{E00A}';
const HWESTA: char = '\u{E00B}';
const ANTO: char = '\u{E00C}';
const AMPA: char = '\u{E00D}';
const ANCA: char = '\u{E00E}';
const UNQUE: char = '\u{E00F}';
const NUMEN: char = '\u{E010}';
const MALTA: char = '\u{E011}';
const NOLDO: char = '\u{E012}';
const NWALME: char = '\u{E013}';
const ROMEN: char = '\u{E014}';
const VALA: char = '\u{E015}';
const ANNA: char = '\u{E016}';
const VILYA: char = '\u{E017}';
const ORE: char = '\u{E020}';
const ARDA: char = '\u{E021}';
const LAMBE: char = '\u{E022}';
const ALDA: char = '\u{E023}';
const SILME: char = '\u{E024}';
const SILME_NUQUERNA: char = '\u{E025}';
const ESSE: char = '\u{E026}';
const ESSE_NUQUERNA: char = '\u{E027}';
const HYARMEN: char = '\u{E028}';
const YANTA: char = '\u{E02A}';
const URE: char = '\u{E02B}';
const LONG_CARRIER: char = '\u{E02C}';
const SHORT_CARRIER: char = '\u{E02E}';

const TEHTA_A: char = '\u{E040}';
const TEHTA_E: char = '\u{E046}';
const TEHTA_I: char = '\u{E044}';
const TEHTA_O: char = '\u{E04A}';
const TEHTA_U: char = '\u{E04C}';
const TEHTA_Y: char = '\u{E043}';
const DOUBLER: char = '\u{E051}';

const PUSTA: char = '\u{E060}';
const DOUBLE_PUSTA: char = '\u{E061}';
const TRIPLE_PUSTA: char = '\u{E062}';
const DASH: char = '\u{E064}';
const EXCLAMATION: char = '\u{E065}';
const QUESTION: char = '\u{E066}';

fn tehta(v: char) -> Option<char> {
    Some(match v {
        'a' => TEHTA_A,
        'e' => TEHTA_E,
        'i' => TEHTA_I,
        'o' => TEHTA_O,
        'u' => TEHTA_U,
        _ => return None,
    })
}

/// (short vowel, long?) for a Latin vowel letter, folding diacritics.
fn vowel(c: char) -> Option<(char, bool)> {
    Some(match c {
        'a' | 'ä' => ('a', false),
        'e' | 'ë' => ('e', false),
        'i' | 'ï' => ('i', false),
        'o' | 'ö' => ('o', false),
        'u' | 'ü' => ('u', false),
        'á' | 'â' | 'ā' => ('a', true),
        'é' | 'ê' | 'ē' => ('e', true),
        'í' | 'î' | 'ī' => ('i', true),
        'ó' | 'ô' | 'ō' => ('o', true),
        'ú' | 'û' | 'ū' => ('u', true),
        _ => return None,
    })
}

fn is_letter(c: char) -> bool {
    c.is_alphabetic() && (c.is_ascii() || matches!(c, 'À'..='ɏ' | 'Ḁ'..='ỿ'))
}

struct Unit {
    /// tengwa (and possible modifier) for a consonant cluster
    glyphs: Vec<char>,
    /// may carry a vowel tehta
    carrier: bool,
}

/// Longest consonant cluster at the start of `s` (lowercase), returned with its length
/// in chars. `before_vowel` tells whether a vowel follows the cluster.
fn consonant(s: &[char], before_vowel: bool) -> Option<(Unit, usize)> {
    let st = |gl: &[char]| Unit { glyphs: gl.to_vec(), carrier: true };
    let pat = |p: &str| -> bool {
        let pc: Vec<char> = p.chars().collect();
        s.len() >= pc.len() && s[..pc.len()] == pc[..]
    };
    // three-letter clusters
    for (p, gl) in [("ngw", vec![UNGWE]), ("nqu", vec![UNQUE]), ("ñw", vec![NWALME])] {
        if pat(p) {
            return Some((st(&gl), p.chars().count()));
        }
    }
    let two: &[(&str, &[char])] = &[
        ("qu", &[QUESSE]),
        ("nd", &[ANDO]),
        ("mb", &[UMBAR]),
        ("ng", &[ANGA]),
        ("nt", &[ANTO]),
        ("mp", &[AMPA]),
        ("nc", &[ANCA]),
        ("nk", &[ANCA]),
        ("hw", &[HWESTA]),
        ("rd", &[ARDA]),
        ("ld", &[ALDA]),
        ("th", &[THULE]),
        ("ss", &[ESSE]),
        ("ty", &[TINCO, TEHTA_Y]),
        ("ly", &[LAMBE, TEHTA_Y]),
        ("ny", &[NUMEN, TEHTA_Y]),
        ("ry", &[ROMEN, TEHTA_Y]),
        ("hy", &[HYARMEN, TEHTA_Y]),
        ("ht", &[AHA, TINCO]),
        ("tt", &[TINCO, DOUBLER]),
        ("pp", &[PARMA, DOUBLER]),
        ("cc", &[CALMA, DOUBLER]),
        ("kk", &[CALMA, DOUBLER]),
        ("ll", &[LAMBE, DOUBLER]),
        ("mm", &[MALTA, DOUBLER]),
        ("nn", &[NUMEN, DOUBLER]),
        ("rr", &[ROMEN, DOUBLER]),
        ("ff", &[FORMEN, DOUBLER]),
        ("vv", &[VALA, DOUBLER]),
        ("ts", &[TINCO, SILME]),
        ("ks", &[CALMA, SILME]),
        ("ch", &[AHA]),
        ("sh", &[AHA]),
        ("ph", &[FORMEN]),
        ("dh", &[ANTO]),
    ];
    for (p, gl) in two {
        if pat(p) {
            return Some((st(gl), 2));
        }
    }
    let c = *s.first()?;
    let gl: Vec<char> = match c {
        't' => vec![TINCO],
        'p' => vec![PARMA],
        'c' | 'k' | 'q' => vec![CALMA],
        'f' => vec![FORMEN],
        'h' => vec![HYARMEN],
        'n' => vec![NUMEN],
        'm' => vec![MALTA],
        'ñ' => vec![NOLDO],
        'r' => vec![if before_vowel { ROMEN } else { ORE }],
        'v' => vec![VALA],
        'y' | 'j' => vec![ANNA],
        'w' => vec![VILYA],
        'l' => vec![LAMBE],
        's' => vec![SILME],
        'z' => vec![ESSE],
        'd' => vec![ANDO],
        'b' => vec![UMBAR],
        'g' => vec![ANGA],
        'x' => vec![CALMA, SILME],
        'þ' => vec![THULE],
        'ð' => vec![ANTO],
        _ => return None,
    };
    Some((st(&gl), 1))
}

fn transcribe_word(word: &str) -> String {
    let chars: Vec<char> = word.to_lowercase().chars().collect();
    let mut out = String::new();
    let mut i = 0;
    // a tengwa waiting for a following vowel's tehta: index in `out` after which the
    // tehta goes (None = nothing to attach to)
    let mut pending: Option<Unit> = None;
    let flush = |out: &mut String, pending: &mut Option<Unit>| {
        if let Some(u) = pending.take() {
            out.extend(u.glyphs.iter());
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if let Some((v, long)) = vowel(c) {
            // diphthong?
            let next = chars.get(i + 1).and_then(|&n| vowel(n)).map(|(n, _)| n);
            let diph = match (v, next) {
                ('a', Some('i')) | ('o', Some('i')) | ('u', Some('i')) => Some(YANTA),
                ('a', Some('u')) | ('e', Some('u')) | ('i', Some('u')) => Some(URE),
                _ => None,
            };
            if let Some(base) = diph {
                if !long {
                    flush(&mut out, &mut pending);
                    out.push(base);
                    out.push(tehta(v).unwrap());
                    i += 2;
                    continue;
                }
            }
            let t = tehta(v).unwrap();
            if long {
                flush(&mut out, &mut pending);
                out.push(LONG_CARRIER);
                out.push(t);
            } else if let Some(u) = pending.take() {
                if u.carrier {
                    // the tehta sits on the preceding consonant; s turns over to make room
                    let mut gl = u.glyphs.clone();
                    if gl.len() == 1 && gl[0] == SILME {
                        gl[0] = SILME_NUQUERNA;
                    } else if gl.len() == 1 && gl[0] == ESSE {
                        gl[0] = ESSE_NUQUERNA;
                    }
                    let last_is_mod = gl.last().map(|&g| g == TEHTA_Y || g == DOUBLER).unwrap_or(false);
                    if last_is_mod {
                        let m = gl.pop().unwrap();
                        out.extend(gl.iter());
                        out.push(t);
                        out.push(m);
                    } else {
                        out.extend(gl.iter());
                        out.push(t);
                    }
                } else {
                    out.extend(u.glyphs.iter());
                    out.push(SHORT_CARRIER);
                    out.push(t);
                }
            } else {
                out.push(SHORT_CARRIER);
                out.push(t);
            }
            i += 1;
            continue;
        }
        // consonant
        flush(&mut out, &mut pending);
        let rest = &chars[i..];
        let mut probe = 1;
        // look ahead past the cluster to know whether a vowel follows (for r/rómen vs órë)
        let before_vowel = |len: usize| rest.get(len).map(|&n| vowel(n).is_some()).unwrap_or(false);
        let unit = consonant(rest, before_vowel(1)).map(|(u, len)| {
            probe = len;
            // r decided with the real cluster length
            if len == 1 && rest[0] == 'r' {
                Unit { glyphs: vec![if before_vowel(1) { ROMEN } else { ORE }], carrier: true }
            } else {
                u
            }
        });
        match unit {
            Some(u) => {
                pending = Some(u);
                i += probe;
            }
            None => {
                out.push(c);
                i += 1;
            }
        }
    }
    flush(&mut out, &mut pending);
    out
}

fn punct(c: char) -> Option<char> {
    Some(match c {
        ',' | ';' | '·' => PUSTA,
        '.' | ':' => DOUBLE_PUSTA,
        '…' => TRIPLE_PUSTA,
        '!' | '¡' => EXCLAMATION,
        '?' | '¿' => QUESTION,
        '-' | '–' | '—' => DASH,
        '0'..='9' => char::from_u32(0xE070 + (c as u32 - '0' as u32)).unwrap(),
        _ => return None,
    })
}

pub fn transcribe(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    let mut word = String::new();
    for c in text.chars() {
        if is_letter(c) {
            word.push(c);
            continue;
        }
        if !word.is_empty() {
            out.push_str(&transcribe_word(&word));
            word.clear();
        }
        match punct(c) {
            Some(p) => out.push(p),
            None => {
                // quotation marks and brackets have no tengwar equivalent
                if !matches!(c, '"' | '“' | '”' | '«' | '»' | '\'' | '‘' | '’') {
                    out.push(c);
                }
            }
        }
    }
    if !word.is_empty() {
        out.push_str(&transcribe_word(&word));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyphs(s: &str) -> Vec<u32> {
        transcribe(s).chars().map(|c| c as u32).collect()
    }

    #[test]
    fn namarie() {
        // "namárië": n+a, m, long carrier+a, r(rómen)+i, short carrier+e
        assert_eq!(glyphs("namárië"), vec![0xE010, 0xE040, 0xE011, 0xE02C, 0xE040, 0xE014, 0xE044, 0xE02E, 0xE046]);
    }

    #[test]
    fn clusters() {
        // "quenya": quessë+e, númen with two dots below +a
        assert_eq!(glyphs("quenya"), vec![0xE003, 0xE046, 0xE010, 0xE040, 0xE043]);
        // "elen": short carrier+e, lambë+e, númen
        assert_eq!(glyphs("elen"), vec![0xE02E, 0xE046, 0xE022, 0xE046, 0xE010]);
        // "aiya": yanta+a, anna+a
        assert_eq!(glyphs("aiya"), vec![0xE02A, 0xE040, 0xE016, 0xE040]);
        // "silmë": silmë nuquerna+i, lambë, malta+e
        assert_eq!(glyphs("silmë"), vec![0xE025, 0xE044, 0xE022, 0xE011, 0xE046]);
        // final r is órë
        assert_eq!(glyphs("nár"), vec![0xE010, 0xE02C, 0xE040, 0xE020]);
    }

    #[test]
    fn punctuation() {
        assert_eq!(glyphs("a, b."), vec![0xE02E, 0xE040, 0xE060, ' ' as u32, 0xE005, 0xE061]);
    }
}
