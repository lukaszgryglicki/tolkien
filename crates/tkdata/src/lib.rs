//! Shared binary formats and text folding used by both `datagen` (writer)
//! and `tolkien` (zero-copy reader).

pub mod features;
pub mod fold;

use std::convert::TryInto;

// ---------------------------------------------------------------- varints

pub fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

/// Decode a varint at `pos`, returning (value, new_pos). Never panics on
/// truncated input: returns what was decoded so far.
#[inline]
pub fn get_varint(buf: &[u8], mut pos: usize) -> (u64, usize) {
    let mut v: u64 = 0;
    let mut shift = 0;
    while pos < buf.len() && shift < 64 {
        let b = buf[pos];
        pos += 1;
        v |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    (v, pos)
}

pub fn put_str(out: &mut Vec<u8>, s: &str) {
    put_varint(out, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

#[inline]
pub fn get_str(buf: &[u8], pos: usize) -> (&str, usize) {
    let (n, p) = get_varint(buf, pos);
    let end = (p + n as usize).min(buf.len());
    (std::str::from_utf8(&buf[p..end]).unwrap_or(""), end)
}

/// A cursor for sequential decoding.
pub struct Reader<'a> {
    pub buf: &'a [u8],
    pub pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }
    #[inline]
    pub fn varint(&mut self) -> u64 {
        let (v, p) = get_varint(self.buf, self.pos);
        self.pos = p;
        v
    }
    #[inline]
    pub fn u8(&mut self) -> u8 {
        let v = self.buf.get(self.pos).copied().unwrap_or(0);
        self.pos += 1;
        v
    }
    #[inline]
    pub fn str(&mut self) -> &'a str {
        let (s, p) = get_str(self.buf, self.pos);
        self.pos = p;
        s
    }
    #[inline]
    pub fn eof(&self) -> bool {
        self.pos >= self.buf.len()
    }
}

// ---------------------------------------------------------------- tables

/// Writer for a `Table`: `u32 count`, `(count+1) u32 offsets`, payload.
#[derive(Default)]
pub struct TableWriter {
    offsets: Vec<u32>,
    payload: Vec<u8>,
}

impl TableWriter {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push(&mut self, item: &[u8]) -> u32 {
        let id = self.offsets.len() as u32;
        self.offsets.push(self.payload.len() as u32);
        self.payload.extend_from_slice(item);
        id
    }
    pub fn len(&self) -> usize {
        self.offsets.len()
    }
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
    pub fn finish(mut self) -> Vec<u8> {
        self.offsets.push(self.payload.len() as u32);
        let mut out = Vec::with_capacity(4 + self.offsets.len() * 4 + self.payload.len());
        out.extend_from_slice(&((self.offsets.len() - 1) as u32).to_le_bytes());
        for o in &self.offsets {
            out.extend_from_slice(&o.to_le_bytes());
        }
        out.extend_from_slice(&self.payload);
        out
    }
}

/// Zero-copy reader for a `Table`.
#[derive(Clone, Copy)]
pub struct Table<'a> {
    count: usize,
    offsets: &'a [u8],
    payload: &'a [u8],
}

impl<'a> Table<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        if buf.len() < 4 {
            return Table { count: 0, offsets: &[], payload: &[] };
        }
        let count = u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize;
        let off_end = 4 + (count + 1) * 4;
        if buf.len() < off_end {
            return Table { count: 0, offsets: &[], payload: &[] };
        }
        Table { count, offsets: &buf[4..off_end], payload: &buf[off_end..] }
    }
    #[inline]
    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    #[inline]
    fn off(&self, i: usize) -> usize {
        u32::from_le_bytes(self.offsets[i * 4..i * 4 + 4].try_into().unwrap()) as usize
    }
    #[inline]
    pub fn get(&self, i: usize) -> Option<&'a [u8]> {
        if i >= self.count {
            return None;
        }
        let a = self.off(i);
        let b = self.off(i + 1);
        if a > b || b > self.payload.len() {
            return None;
        }
        Some(&self.payload[a..b])
    }
}

// ---------------------------------------------------------------- zstd block store

/// Items are grouped into blocks of `per_block` items; each block is a zstd
/// frame containing the concatenated length-prefixed items.
///
/// Layout of the index buffer: `u32 per_block`, `u32 n_items`, `(n_blocks+1) u32 offsets`.
pub struct BlockWriter {
    per_block: usize,
    n_items: usize,
    cur: Vec<u8>,
    cur_n: usize,
    offsets: Vec<u32>,
    data: Vec<u8>,
    level: i32,
}

impl BlockWriter {
    pub fn new(per_block: usize, level: i32) -> Self {
        BlockWriter { per_block, n_items: 0, cur: Vec::new(), cur_n: 0, offsets: vec![0], data: Vec::new(), level }
    }
    pub fn push(&mut self, item: &[u8]) -> u64 {
        let id = self.n_items as u64;
        put_varint(&mut self.cur, item.len() as u64);
        self.cur.extend_from_slice(item);
        self.cur_n += 1;
        self.n_items += 1;
        if self.cur_n == self.per_block {
            self.flush();
        }
        id
    }
    fn flush(&mut self) {
        if self.cur_n == 0 {
            return;
        }
        let comp = zstd::bulk::compress(&self.cur, self.level).expect("zstd compress");
        self.data.extend_from_slice(&comp);
        self.offsets.push(self.data.len() as u32);
        self.cur.clear();
        self.cur_n = 0;
    }
    /// Returns (index, data).
    pub fn finish(mut self) -> (Vec<u8>, Vec<u8>) {
        self.flush();
        let mut idx = Vec::with_capacity(8 + self.offsets.len() * 4);
        idx.extend_from_slice(&(self.per_block as u32).to_le_bytes());
        idx.extend_from_slice(&(self.n_items as u32).to_le_bytes());
        for o in &self.offsets {
            idx.extend_from_slice(&o.to_le_bytes());
        }
        (idx, self.data)
    }
}

/// Reader with a tiny LRU cache of decompressed blocks.
pub struct BlockReader<'a> {
    per_block: usize,
    n_items: usize,
    offsets: &'a [u8],
    data: &'a [u8],
    cache: std::cell::RefCell<Vec<(usize, Vec<u8>)>>,
}

impl<'a> BlockReader<'a> {
    pub fn new(idx: &'a [u8], data: &'a [u8]) -> Self {
        if idx.len() < 8 {
            return BlockReader { per_block: 1, n_items: 0, offsets: &[], data: &[], cache: Default::default() };
        }
        let per_block = u32::from_le_bytes(idx[0..4].try_into().unwrap()) as usize;
        let n_items = u32::from_le_bytes(idx[4..8].try_into().unwrap()) as usize;
        BlockReader { per_block: per_block.max(1), n_items, offsets: &idx[8..], data, cache: Default::default() }
    }
    pub fn len(&self) -> usize {
        self.n_items
    }
    pub fn is_empty(&self) -> bool {
        self.n_items == 0
    }
    fn off(&self, i: usize) -> Option<usize> {
        let s = self.offsets.get(i * 4..i * 4 + 4)?;
        Some(u32::from_le_bytes(s.try_into().unwrap()) as usize)
    }
    /// Run `f` on the decompressed block containing `id`, passing the index of
    /// the item within the block. Items are varint-length-prefixed.
    pub fn with_block<R>(&self, id: usize, f: impl FnOnce(&[u8], usize) -> R) -> Option<R> {
        if id >= self.n_items {
            return None;
        }
        let block = id / self.per_block;
        let within = id % self.per_block;
        let mut cache = self.cache.borrow_mut();
        let pos = cache.iter().position(|(b, _)| *b == block);
        let buf: &Vec<u8> = match pos {
            Some(p) => {
                if p != 0 {
                    let e = cache.remove(p);
                    cache.insert(0, e);
                }
                &cache[0].1
            }
            None => {
                let a = self.off(block)?;
                let b = self.off(block + 1)?;
                if a > b || b > self.data.len() {
                    return None;
                }
                let raw = zstd::bulk::decompress(&self.data[a..b], 1 << 22).ok()?;
                cache.insert(0, (block, raw));
                if cache.len() > 64 {
                    cache.pop();
                }
                &cache[0].1
            }
        };
        Some(f(buf, within))
    }

    /// Run `f` on the raw bytes of item `id`.
    pub fn with_item<R>(&self, id: usize, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
        self.with_block(id, |buf, within| {
            let mut pos = 0usize;
            for _ in 0..within {
                let (n, p) = get_varint(buf, pos);
                pos = p + n as usize;
                if pos > buf.len() {
                    return None;
                }
            }
            let (n, p) = get_varint(buf, pos);
            let end = p + n as usize;
            if end > buf.len() {
                return None;
            }
            Some(f(&buf[p..end]))
        })?
    }
}

/// Weight quantisation levels (3 bits) packed with a POS code (5 bits).
/// Quantised candidate weights (4 bits). The top levels separate translation-table
/// links (90), links confirmed by an early gloss (91-94) and their inflected forms.
const WEIGHT_LEVELS: [u8; 16] = [25, 40, 50, 58, 65, 71, 75, 78, 81, 84, 86, 88, 90, 91, 92, 94];

#[inline]
pub fn pack_wp(weight: u8, pos: u8) -> u8 {
    let mut q = 0u8;
    for (i, lvl) in WEIGHT_LEVELS.iter().enumerate() {
        if weight >= *lvl {
            q = i as u8;
        }
    }
    // rare parts of speech (suffix, prefix, character, other) share one code
    let p = if pos >= 14 { 14 } else { pos };
    (q << 4) | p
}

/// Returns (weight, pos).
#[inline]
pub fn unpack_wp(b: u8) -> (u8, u8) {
    (WEIGHT_LEVELS[(b >> 4) as usize], b & 0x0f)
}

#[inline]
pub fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

#[inline]
pub fn unzigzag(v: u64) -> i64 {
    ((v >> 1) as i64) ^ -((v & 1) as i64)
}

// ---------------------------------------------------------------- enums

/// Part of speech codes shared by all tables.
pub mod pos {
    pub const UNKNOWN: u8 = 0;
    pub const NOUN: u8 = 1;
    pub const NAME: u8 = 2;
    pub const VERB: u8 = 3;
    pub const ADJ: u8 = 4;
    pub const ADV: u8 = 5;
    pub const PRON: u8 = 6;
    pub const NUM: u8 = 7;
    pub const DET: u8 = 8;
    pub const PREP: u8 = 9;
    pub const CONJ: u8 = 10;
    pub const INTERJ: u8 = 11;
    pub const PART: u8 = 12;
    pub const PHRASE: u8 = 13;
    pub const SUF: u8 = 14;
    pub const PREF: u8 = 15;
    pub const CHAR: u8 = 16;
    pub const OTHER: u8 = 17;

    pub fn from_str(s: &str) -> u8 {
        // Eldamo uses compound speech values like "adj pron"; take the first token.
        let first = s.split_whitespace().next().unwrap_or("");
        match first {
            "n" | "collective-noun" => NOUN,
            "name" | "masc-name" | "fem-name" | "place-name" | "proper-name" | "collective-name" => NAME,
            "vb" => VERB,
            "adj" => ADJ,
            "adv" => ADV,
            "pron" => PRON,
            "num" | "cardinal" | "ordinal" | "fraction" => NUM,
            "det" | "article" => DET,
            "prep" => PREP,
            "conj" => CONJ,
            "interj" => INTERJ,
            "part" | "particle" => PART,
            "phrase" => PHRASE,
            "suf" => SUF,
            "pref" => PREF,
            "char" => CHAR,
            "" => UNKNOWN,
            _ => OTHER,
        }
    }

    pub fn name(p: u8) -> &'static str {
        match p {
            NOUN => "n",
            NAME => "name",
            VERB => "vb",
            ADJ => "adj",
            ADV => "adv",
            PRON => "pron",
            NUM => "num",
            DET => "det",
            PREP => "prep",
            CONJ => "conj",
            INTERJ => "interj",
            PART => "part",
            PHRASE => "phrase",
            SUF => "suf",
            PREF => "pref",
            CHAR => "char",
            OTHER => "other",
            _ => "?",
        }
    }
}

/// Quenya verb class codes.
pub mod vclass {
    pub const NONE: u8 = 0;
    pub const BASIC: u8 = 1;
    pub const TA_FORMATIVE: u8 = 2;
    pub const YA_FORMATIVE: u8 = 3;
    pub const A_VERB: u8 = 4;
    pub const U_VERB: u8 = 5;
    pub const WEAK: u8 = 6;
    pub const NA_FORMATIVE: u8 = 7;
    pub const TA_CAUSATIVE: u8 = 8;
    pub const IRREGULAR: u8 = 9;
    pub const TALAT_STEM: u8 = 10;
    pub const YA_CAUSATIVE: u8 = 11;

    pub fn from_str(s: &str) -> u8 {
        match s {
            "basic-verb" => BASIC,
            "ta-formative" => TA_FORMATIVE,
            "ya-formative" => YA_FORMATIVE,
            "a-verb" => A_VERB,
            "u-verb" => U_VERB,
            "weak-verb" => WEAK,
            "na-formative" => NA_FORMATIVE,
            "ta-causative" => TA_CAUSATIVE,
            "irregular-verb" => IRREGULAR,
            "talat-stem" => TALAT_STEM,
            "ya-causative" => YA_CAUSATIVE,
            _ => NONE,
        }
    }
}

/// Elvish source-language codes.
pub mod elang {
    pub const Q: u8 = 0; // late Quenya
    pub const MQ: u8 = 1; // Middle Quenya
    pub const NQ: u8 = 2; // Neo-Quenya
    pub const S: u8 = 3; // Sindarin
    pub const N: u8 = 4; // Noldorin
    pub const NS: u8 = 5; // Neo-Sindarin
    pub fn from_str(s: &str) -> u8 {
        match s {
            "q" => Q,
            "mq" => MQ,
            "nq" => NQ,
            "s" => S,
            "n" => N,
            "ns" => NS,
            _ => Q,
        }
    }
    pub fn name(l: u8) -> &'static str {
        match l {
            Q => "Q",
            MQ => "ᴹQ",
            NQ => "ᴺQ",
            S => "S",
            N => "N",
            NS => "ᴺS",
            _ => "?",
        }
    }
}

/// Bit flags for Eldamo marks on an entry.
pub mod mark {
    pub const NEOLOGISM: u8 = 1; // !
    pub const ADAPTED: u8 = 2; // ^
    pub const ARCHAIC: u8 = 4; // †
    pub const QUESTIONABLE: u8 = 8; // *
    pub const DERIVED: u8 = 16; // #
    pub const UNCERTAIN: u8 = 32; // ?
    pub const NESTED: u8 = 64; // older-period predecessor entry
    pub fn from_str(s: &str) -> u8 {
        let mut m = 0;
        for c in s.chars() {
            m |= match c {
                '!' => NEOLOGISM,
                '^' => ADAPTED,
                '†' => ARCHAIC,
                '*' => QUESTIONABLE,
                '#' => DERIVED,
                '?' => UNCERTAIN,
                _ => 0,
            };
        }
        m
    }
}

/// Link kinds in the English graph.
pub mod link {
    pub const FORM: u8 = 0;
    pub const ALIAS: u8 = 1;
}
