//! Zero-copy access to the embedded dictionaries.

use fst::Map;
use std::cell::RefCell;
use tkdata::{get_str, get_varint, unpack_wp, unzigzag, BlockReader, Reader, Table};

macro_rules! blob {
    ($name:literal) => {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/", $name))
    };
}

static EN_FST: &[u8] = blob!("en.fst");
static EN_IDX: &[u8] = blob!("en.idx");
static EN_BLK: &[u8] = blob!("en.blk");
static EN_KEYS_IDX: &[u8] = blob!("en_keys.idx");
static EN_KEYS_BLK: &[u8] = blob!("en_keys.blk");
static QUENYA: &[u8] = blob!("quenya.bin");
static SINDARIN: &[u8] = blob!("sindarin.bin");
static ERANK: &[u8] = blob!("erank.bin");
static LEX_FST: &[u8] = blob!("lex.fst");
static LEX_IDX: &[u8] = blob!("lex.idx");
static LEX_BLK: &[u8] = blob!("lex.blk");
static LISTS_IDX: &[u8] = blob!("lists.idx");
static LISTS_BLK: &[u8] = blob!("lists.blk");
static TAGSETS: &[u8] = blob!("tagsets.bin");
static LANGS: &[u8] = blob!("langs.bin");

/// One candidate English lemma for a source-language word.
#[derive(Clone, Debug)]
pub struct LexCand {
    pub en: u32,
    pub weight: u8,
    pub pos: u8,
    /// Possible analyses (feature bit sets); empty or [0] for a base form.
    pub tagsets: Vec<u64>,
}

#[derive(Clone, Debug)]
pub struct EnLink {
    pub target: u32,
    pub kind: u8,
    pub weight: u8,
    pub tags: u64,
}

#[derive(Clone, Debug, Default)]
pub struct EnEntry {
    pub quenya: Vec<(u32, u8)>,
    pub sindarin: Vec<(u32, u8)>,
    pub links: Vec<EnLink>,
}

/// A decoded Elvish dictionary entry.
#[derive(Clone, Debug)]
#[allow(dead_code)] // every field of the on-disk record is decoded; not all are consulted yet
pub struct Entry {
    pub id: u32,
    pub form: &'static str,
    pub stem: &'static str,
    pub pos: u8,
    pub lang: u8,
    pub vclass: u8,
    pub mark: u8,
    pub refs: u8,
    pub gloss: &'static str,
    pub infl: Vec<(&'static str, &'static str)>,
    pub tengwar: &'static str,
    pub speech: &'static str,
}

impl Entry {
    pub fn attested(&self, kind: &str) -> Option<&'static str> {
        self.infl.iter().find(|(k, _)| *k == kind).map(|(_, f)| *f)
    }
}

pub struct Data {
    pub en_fst: Map<&'static [u8]>,
    en_blk: BlockReader<'static>,
    en_keys: BlockReader<'static>,
    quenya: Table<'static>,
    sindarin: Table<'static>,
    pub lex_fst: Map<&'static [u8]>,
    lex_blk: BlockReader<'static>,
    lists: BlockReader<'static>,
    langs: Table<'static>,
    lex_cache: RefCell<std::collections::HashMap<String, Option<Vec<LexCand>>>>,
    /// language detection: (lang, word) → evidence weight of a dictionary hit
    pub hit_cache: RefCell<std::collections::HashMap<String, Option<f32>>>,
}

impl Data {
    pub fn load() -> Data {
        Data {
            en_fst: Map::new(EN_FST).expect("en.fst"),
            en_blk: BlockReader::new(EN_IDX, EN_BLK),
            en_keys: BlockReader::new(EN_KEYS_IDX, EN_KEYS_BLK),
            quenya: Table::new(QUENYA),
            sindarin: Table::new(SINDARIN),
            lex_fst: Map::new(LEX_FST).expect("lex.fst"),
            lex_blk: BlockReader::new(LEX_IDX, LEX_BLK),
            lists: BlockReader::new(LISTS_IDX, LISTS_BLK),
            langs: Table::new(LANGS),
            lex_cache: RefCell::new(std::collections::HashMap::new()),
            hit_cache: RefCell::new(std::collections::HashMap::new()),
        }
    }

    // ------------------------------------------------------------ English

    pub fn en_id(&self, key: &str) -> Option<u32> {
        self.en_fst.get(key.as_bytes()).map(|v| v as u32)
    }

    pub fn en_key(&self, id: u32) -> String {
        self.en_keys
            .with_item(id as usize, |b| String::from_utf8_lossy(b).to_string())
            .unwrap_or_default()
    }

    pub fn en_entry(&self, id: u32) -> EnEntry {
        self.en_blk
            .with_item(id as usize, |b| {
                let mut r = Reader::new(b);
                let mut e = EnEntry::default();
                for list in [&mut e.quenya, &mut e.sindarin] {
                    let n = r.varint() as usize;
                    for _ in 0..n.min(64) {
                        let id = r.varint() as u32;
                        let sc = r.u8();
                        list.push((id, sc));
                    }
                }
                let n = r.varint() as usize;
                for _ in 0..n.min(64) {
                    let target = r.varint() as u32;
                    let kind = r.u8();
                    let weight = r.u8();
                    let tags = r.varint();
                    e.links.push(EnLink { target, kind, weight, tags });
                }
                e
            })
            .unwrap_or_default()
    }

    // ------------------------------------------------------------ Elvish

    fn decode_entry(buf: &'static [u8], id: u32) -> Entry {
        let mut r = Reader::new(buf);
        let form = r.str();
        let stem = r.str();
        let pos = r.u8();
        let lang = r.u8();
        let vclass = r.u8();
        let mark = r.u8();
        let refs = r.u8();
        let gloss = r.str();
        let n = r.varint() as usize;
        let mut infl = Vec::with_capacity(n.min(64));
        for _ in 0..n.min(256) {
            let k = r.str();
            let f = r.str();
            infl.push((k, f));
        }
        let tengwar = r.str();
        let speech = r.str();
        Entry { id, form, stem, pos, lang, vclass, mark, refs, gloss, infl, tengwar, speech }
    }

    pub fn quenya(&self, id: u32) -> Option<Entry> {
        self.quenya.get(id as usize).map(|b| Self::decode_entry(b, id))
    }

    pub fn sindarin(&self, id: u32) -> Option<Entry> {
        self.sindarin.get(id as usize).map(|b| Self::decode_entry(b, id))
    }

    pub fn elvish(&self, target: Target, id: u32) -> Option<Entry> {
        match target {
            Target::Quenya => self.quenya(id),
            Target::Sindarin => self.sindarin(id),
        }
    }

    // ------------------------------------------------------------ multilingual lexicon

    fn erank(&self, rank: u64) -> Option<u32> {
        let i = rank as usize * 4;
        ERANK.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn tagset(&self, id: u64) -> u64 {
        let i = id as usize * 8;
        TAGSETS
            .get(i..i + 8)
            .map(|b| u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
            .unwrap_or(0)
    }

    fn list(&self, list_id: u64) -> Vec<(u32, u8, u8)> {
        self.lists
            .with_item(list_id as usize, |b| {
                let mut r = Reader::new(b);
                let n = r.varint() as usize;
                let mut out = Vec::with_capacity(n.min(16));
                for _ in 0..n.min(16) {
                    let rank = r.varint();
                    let (w, p) = unpack_wp(r.u8());
                    if let Some(en) = self.erank(rank) {
                        out.push((en, w, p));
                    }
                }
                out
            })
            .unwrap_or_default()
    }

    /// Look up a folded word in the given language.
    pub fn lex_lookup(&self, lang: &str, folded: &str) -> Option<Vec<LexCand>> {
        let cache_key = format!("{}\u{1}{}", lang, folded);
        if let Some(v) = self.lex_cache.borrow().get(&cache_key) {
            return v.clone();
        }
        let result = self.lex_lookup_uncached(cache_key.as_bytes());
        let mut cache = self.lex_cache.borrow_mut();
        if cache.len() >= 16384 {
            cache.clear();
        }
        cache.insert(cache_key, result.clone());
        result
    }

    /// Lexicon keys of `lang` starting with `prefix` (at most `limit`).
    pub fn lex_prefix_keys(&self, lang: &str, prefix: &str, limit: usize) -> Vec<String> {
        use fst::{IntoStreamer, Streamer};
        let mut lo = Vec::with_capacity(lang.len() + 1 + prefix.len());
        lo.extend_from_slice(lang.as_bytes());
        lo.push(1);
        lo.extend_from_slice(prefix.as_bytes());
        let mut hi = lo.clone();
        hi.push(0xff);
        let mut stream = self.lex_fst.range().ge(&lo).lt(&hi).into_stream();
        let mut out = Vec::new();
        while let Some((k, _)) = stream.next() {
            if out.len() >= limit {
                break;
            }
            if let Ok(s) = std::str::from_utf8(&k[lang.len() + 1..]) {
                out.push(s.to_string());
            }
        }
        out
    }

    /// True if the (lang, folded word) key exists (cheap, no decoding).
    pub fn lex_has(&self, lang: &str, folded: &str) -> bool {
        let mut key = Vec::with_capacity(lang.len() + 1 + folded.len());
        key.extend_from_slice(lang.as_bytes());
        key.push(1);
        key.extend_from_slice(folded.as_bytes());
        self.lex_fst.contains_key(&key)
    }

    fn lex_lookup_uncached(&self, key: &[u8]) -> Option<Vec<LexCand>> {
        let ordinal = self.lex_fst.get(key)? as usize;
        self.lex_blk.with_block(ordinal, |buf, within| {
            // walk the block, tracking the delta coded list id
            let mut pos = 0usize;
            let mut list_id: i64 = 0;
            let mut item: &[u8] = &[];
            for i in 0..=within {
                let (n, p) = get_varint(buf, pos);
                let end = p + n as usize;
                if end > buf.len() {
                    return None;
                }
                item = &buf[p..end];
                let (raw, _) = get_varint(item, 0);
                list_id = if i == 0 { raw as i64 } else { list_id + unzigzag(raw) };
                pos = end;
            }
            if list_id < 0 {
                return None;
            }
            let (_, mut p) = get_varint(item, 0);
            let mode = item.get(p).copied().unwrap_or(0);
            p += 1;
            let list = self.list(list_id as u64);
            let mut cands: Vec<LexCand> = list
                .iter()
                .map(|(en, w, pos)| LexCand { en: *en, weight: *w, pos: *pos, tagsets: Vec::new() })
                .collect();
            match mode {
                1 => {
                    let (n, mut q) = get_varint(item, p);
                    let mut ts = Vec::new();
                    for _ in 0..n.min(8) {
                        let (t, q2) = get_varint(item, q);
                        q = q2;
                        ts.push(self.tagset(t));
                    }
                    for c in &mut cands {
                        c.tagsets = ts.clone();
                    }
                }
                2 => {
                    let mut q = p;
                    for c in &mut cands {
                        let (n, q1) = get_varint(item, q);
                        q = q1;
                        for _ in 0..n.min(8) {
                            let (t, q2) = get_varint(item, q);
                            q = q2;
                            c.tagsets.push(self.tagset(t));
                        }
                    }
                }
                _ => {
                    for c in &mut cands {
                        c.tagsets.push(0);
                    }
                }
            }
            Some(cands)
        })?
    }

    /// All language codes present in the lexicon with their key counts.
    pub fn langs(&self) -> Vec<(String, u64)> {
        let mut out = Vec::with_capacity(self.langs.len());
        for i in 0..self.langs.len() {
            if let Some(b) = self.langs.get(i) {
                let (code, p) = get_str(b, 0);
                let (n, _) = get_varint(b, p);
                out.push((code.to_string(), n));
            }
        }
        out
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Quenya,
    Sindarin,
}
