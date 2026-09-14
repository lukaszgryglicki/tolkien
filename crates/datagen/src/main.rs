//! datagen: compiles the research outputs into the compact binary blobs that
//! are embedded into the `tolkien` binary.
//!
//! Usage: datagen <research/build dir> <tools dir> <out data dir>

use fst::MapBuilder;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use tkdata::features;
use tkdata::{elang, link, mark, pos, put_str, put_varint, vclass, BlockWriter, TableWriter};

#[derive(Deserialize)]
struct Lexicon {
    entries: Vec<Entry>,
    names: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    form: String,
    stem: Option<String>,
    pos: String,
    lang: String,
    mark: String,
    nested: bool,
    vclass: Option<String>,
    tengwar: Option<String>,
    refs: u32,
    gloss: String,
    keys: Vec<Key>,
    infl: HashMap<String, Vec<String>>,
}

#[derive(Deserialize)]
struct Key {
    key: String,
    flags: Vec<String>,
    group: u32,
    item: u32,
}

fn score(e: &Entry, k: &Key, is_name: bool) -> i32 {
    let mut s: i32 = match e.lang.as_str() {
        "q" | "s" => 100,
        "mq" | "n" => 85,
        _ => 75,
    };
    for c in e.mark.chars() {
        s += match c {
            '!' => -5,
            '^' => -10,
            '†' => -30,
            '*' => -15,
            '?' => -20,
            _ => 0,
        };
    }
    if e.nested {
        s -= 10;
    }
    for f in &k.flags {
        s += match f.as_str() {
            "warn" => -45,
            "period:ᴱQ." | "period:ᴱN." | "period:G." => -25,
            "period:ᴹQ." | "period:N." => -8,
            _ => 0,
        };
    }
    s -= 6 * k.group as i32 + 3 * k.item as i32;
    s += (e.refs as i32).min(15);
    if e.form.contains(' ') {
        s -= 20;
    }
    if is_name {
        s -= 5;
    }
    s.clamp(1, 127)
}

fn encode_entry(e: &Entry, is_name: bool) -> Vec<u8> {
    let mut b = Vec::new();
    put_str(&mut b, &e.form);
    put_str(&mut b, e.stem.as_deref().unwrap_or(""));
    b.push(if is_name { pos::NAME } else { pos::from_str(&e.pos) });
    b.push(elang::from_str(&e.lang));
    b.push(vclass::from_str(e.vclass.as_deref().unwrap_or("")));
    let mut m = mark::from_str(&e.mark);
    if e.nested {
        m |= mark::NESTED;
    }
    b.push(m);
    b.push(e.refs.min(255) as u8);
    put_str(&mut b, &e.gloss);
    let mut infl: Vec<(&String, &Vec<String>)> = e.infl.iter().collect();
    infl.sort();
    let n: usize = infl.iter().map(|(_, v)| v.len()).sum();
    put_varint(&mut b, n as u64);
    for (kind, forms) in infl {
        for f in forms {
            put_str(&mut b, kind);
            put_str(&mut b, f);
        }
    }
    put_str(&mut b, e.tengwar.as_deref().unwrap_or(""));
    put_str(&mut b, &e.pos);
    b
}

struct ElvishTables {
    table: Vec<u8>,
    /// english key -> [(entry id, score)]
    by_key: HashMap<String, Vec<(u32, u8)>>,
}

fn build_elvish(path: &str) -> ElvishTables {
    let lex: Lexicon = serde_json::from_reader(BufReader::new(File::open(path).expect(path))).expect("lexicon json");
    let mut tw = TableWriter::new();
    let mut by_key: HashMap<String, Vec<(u32, u8)>> = HashMap::new();
    for (is_name, list) in [(false, &lex.entries), (true, &lex.names)] {
        for e in list {
            let id = tw.push(&encode_entry(e, is_name));
            for k in &e.keys {
                if k.key.contains(':') || k.key.is_empty() || k.key.len() > 60 {
                    continue;
                }
                let key = tkdata::fold::fold_en(&k.key);
                if key.is_empty() {
                    continue;
                }
                by_key.entry(key).or_default().push((id, score(e, k, is_name) as u8));
            }
        }
    }
    for v in by_key.values_mut() {
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v.dedup_by_key(|x| x.0);
        v.truncate(10);
    }
    eprintln!("{}: {} entries, {} english keys", path, tw.len(), by_key.len());
    ElvishTables { table: tw.finish(), by_key }
}

struct Link {
    target: String,
    kind: u8,
    weight: u8,
    tags: u64,
}

fn read_links(path: &str) -> HashMap<String, Vec<Link>> {
    let mut out: HashMap<String, Vec<Link>> = HashMap::new();
    let f = BufReader::new(File::open(path).expect(path));
    for line in f.lines() {
        let line = line.unwrap();
        let mut it = line.split('\t');
        let (Some(src), Some(kind), Some(target), Some(tags), Some(weight)) =
            (it.next(), it.next(), it.next(), it.next(), it.next())
        else {
            continue;
        };
        let kind = if kind == "form" { link::FORM } else { link::ALIAS };
        let weight: u8 = weight.parse().unwrap_or(50);
        out.entry(src.to_string()).or_default().push(Link {
            target: target.to_string(),
            kind,
            weight,
            tags: features::parse(tags),
        });
    }
    eprintln!("{}: {} sources", path, out.len());
    out
}

const LEX_BLOCK: u64 = 256;

fn write(path: &str, data: &[u8]) {
    let mut f = BufWriter::new(File::create(path).expect(path));
    f.write_all(data).unwrap();
    eprintln!("wrote {} ({} bytes)", path, data.len());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: datagen <build dir> <tools dir> <out dir>");
        std::process::exit(2);
    }
    let build = &args[1];
    let tools = &args[2];
    let out = &args[3];
    std::fs::create_dir_all(out).unwrap();

    let q = build_elvish(&format!("{}/lexicon_q.json", build));
    let s = build_elvish(&format!("{}/lexicon_s.json", build));
    let links = read_links(&format!("{}/en_links.tsv", build));

    // ---- English key universe
    let mut keys: Vec<String> = Vec::new();
    keys.extend(q.by_key.keys().cloned());
    keys.extend(s.by_key.keys().cloned());
    for (src, ls) in &links {
        keys.push(src.clone());
        for l in ls {
            keys.push(l.target.clone());
        }
    }
    for line in BufReader::new(File::open(format!("{}/function_words.txt", tools)).unwrap()).lines() {
        let w = tkdata::fold::fold_en(&line.unwrap());
        if !w.is_empty() && !w.starts_with('#') {
            keys.push(w);
        }
    }
    keys.sort();
    keys.dedup();
    let en_id: HashMap<&str, u32> = keys.iter().enumerate().map(|(i, k)| (k.as_str(), i as u32)).collect();
    eprintln!("english keys: {}", keys.len());

    // en.fst
    let mut mb = MapBuilder::memory();
    for (i, k) in keys.iter().enumerate() {
        mb.insert(k.as_bytes(), i as u64).unwrap();
    }
    let en_fst = mb.into_inner().unwrap();
    write(&format!("{}/en.fst", out), &en_fst);

    // en.blk / en.idx
    let mut tw = BlockWriter::new(64, 9);
    let empty: Vec<(u32, u8)> = Vec::new();
    let no_links: Vec<Link> = Vec::new();
    for k in &keys {
        let mut b = Vec::new();
        for tab in [&q.by_key, &s.by_key] {
            let c = tab.get(k).unwrap_or(&empty);
            put_varint(&mut b, c.len() as u64);
            for (id, sc) in c {
                put_varint(&mut b, *id as u64);
                b.push(*sc);
            }
        }
        let ls = links.get(k).unwrap_or(&no_links);
        put_varint(&mut b, ls.len() as u64);
        for l in ls {
            put_varint(&mut b, en_id[l.target.as_str()] as u64);
            b.push(l.kind);
            b.push(l.weight);
            put_varint(&mut b, l.tags);
        }
        tw.push(&b);
    }
    let (idx, data) = tw.finish();
    write(&format!("{}/en.idx", out), &idx);
    write(&format!("{}/en.blk", out), &data);
    write(&format!("{}/quenya.bin", out), &q.table);
    write(&format!("{}/sindarin.bin", out), &s.table);

    // en_keys.blk: id -> key string (only used by --explain)
    let mut tw = BlockWriter::new(256, 9);
    for k in &keys {
        tw.push(k.as_bytes());
    }
    let (idx, data) = tw.finish();
    write(&format!("{}/en_keys.idx", out), &idx);
    write(&format!("{}/en_keys.blk", out), &data);

    // ---- multilingual lexicon
    let lex_path = format!("{}/lex.tsv", build);
    // pass 1: english key frequencies -> rank ids
    let mut freq: Vec<u32> = vec![0; keys.len()];
    let mut tag_freq: HashMap<u64, u64> = HashMap::new();
    let mut n_lines = 0usize;
    for line in BufReader::new(File::open(&lex_path).unwrap()).lines() {
        let line = line.unwrap();
        n_lines += 1;
        for cand in line.split('\t').skip(1) {
            let mut it = cand.split('|');
            let e = it.next().unwrap_or("");
            if let Some(&id) = en_id.get(e) {
                freq[id as usize] += 1;
            }
            if let Some(ts) = it.nth(2) {
                for t in ts.split(';') {
                    *tag_freq.entry(features::parse(t)).or_default() += 1;
                }
            }
        }
    }
    // tagset ids by descending frequency so that common ones take one byte
    let mut tagsets: Vec<u64> = tag_freq.keys().copied().collect();
    tagsets.sort_by(|a, b| tag_freq[b].cmp(&tag_freq[a]).then(a.cmp(b)));
    let tagset_ids: HashMap<u64, u32> = tagsets.iter().enumerate().map(|(i, t)| (*t, i as u32)).collect();
    let mut order: Vec<u32> = (0..keys.len() as u32).collect();
    order.sort_by(|a, b| freq[*b as usize].cmp(&freq[*a as usize]).then(a.cmp(b)));
    let mut rank_of: Vec<u32> = vec![0; keys.len()];
    for (r, id) in order.iter().enumerate() {
        rank_of[*id as usize] = r as u32;
    }
    let mut erank = Vec::with_capacity(order.len() * 4);
    for id in &order {
        erank.extend_from_slice(&id.to_le_bytes());
    }
    write(&format!("{}/erank.bin", out), &erank);
    eprintln!("lex lines: {}", n_lines);

    // pass 2: build fst + values
    let mut list_ids: HashMap<Vec<u8>, u32> = HashMap::new();
    let mut lists = BlockWriter::new(64, 9);
    let mut values = BlockWriter::new(LEX_BLOCK as usize, 9);
    let mut prev_list: u32 = 0;
    let mut lang_ids: HashMap<String, u32> = HashMap::new();
    let mut langs: Vec<(String, u32)> = Vec::new();
    let mut mb = MapBuilder::memory();
    let mut prev_key: Vec<u8> = Vec::new();
    let mut ordinal: u64 = 0;
    let mut skipped = 0usize;
    for line in BufReader::new(File::open(&lex_path).unwrap()).split(b'\n') {
        let line = line.unwrap();
        let Some(tab) = line.iter().position(|&b| b == b'\t') else { continue };
        let key = &line[..tab];
        if !prev_key.is_empty() && key <= prev_key.as_slice() {
            skipped += 1;
            continue;
        }
        let Ok(rest) = std::str::from_utf8(&line[tab + 1..]) else { continue };
        let Some(sep) = key.iter().position(|&b| b == 1) else { continue };
        let lang = String::from_utf8_lossy(&key[..sep]).to_string();
        // candidates
        let mut list = Vec::new();
        let mut cand_tags: Vec<Vec<u32>> = Vec::new();
        for cand in rest.split('\t') {
            let mut it = cand.split('|');
            let (Some(e), Some(w), Some(p), Some(ts)) = (it.next(), it.next(), it.next(), it.next()) else {
                continue;
            };
            let Some(&id) = en_id.get(e) else { continue };
            let w: u8 = w.parse().unwrap_or(50);
            let p = pos::from_str(p);
            put_varint(&mut list, rank_of[id as usize] as u64);
            list.push(tkdata::pack_wp(w, p));
            let mut tids = Vec::new();
            for t in ts.split(';') {
                let tid = tagset_ids[&features::parse(t)];
                if !tids.contains(&tid) {
                    tids.push(tid);
                }
            }
            cand_tags.push(tids);
        }
        if cand_tags.is_empty() {
            skipped += 1;
            continue;
        }
        let mut list_full = Vec::new();
        put_varint(&mut list_full, cand_tags.len() as u64);
        list_full.extend_from_slice(&list);
        let list_id = match list_ids.get(&list_full) {
            Some(&id) => id,
            None => {
                let id = lists.push(&list_full) as u32;
                list_ids.insert(list_full, id);
                id
            }
        };
        let mut val = Vec::new();
        // list ids are delta coded within a block (forms of one lemma are adjacent)
        if ordinal % LEX_BLOCK == 0 {
            put_varint(&mut val, list_id as u64);
        } else {
            put_varint(&mut val, tkdata::zigzag(list_id as i64 - prev_list as i64));
        }
        prev_list = list_id;
        let all_base = cand_tags.iter().all(|t| t.len() == 1 && tagsets[t[0] as usize] == 0);
        let shared = cand_tags.iter().all(|t| *t == cand_tags[0]);
        if all_base {
            val.push(0);
        } else if shared {
            val.push(1);
            put_varint(&mut val, cand_tags[0].len() as u64);
            for t in &cand_tags[0] {
                put_varint(&mut val, *t as u64);
            }
        } else {
            val.push(2);
            for tids in &cand_tags {
                put_varint(&mut val, tids.len() as u64);
                for t in tids {
                    put_varint(&mut val, *t as u64);
                }
            }
        }
        let vid = values.push(&val);
        debug_assert_eq!(vid, ordinal);
        mb.insert(key, ordinal).unwrap();
        ordinal += 1;
        prev_key = key.to_vec();
        let n = langs.len() as u32;
        let lid = *lang_ids.entry(lang.clone()).or_insert_with(|| {
            langs.push((lang.clone(), 0));
            n
        });
        langs[lid as usize].1 += 1;
    }
    eprintln!("lex keys: {} skipped: {} lists: {} tagsets: {} langs: {}", ordinal, skipped, list_ids.len(), tagsets.len(), langs.len());
    let lex_fst = mb.into_inner().unwrap();
    write(&format!("{}/lex.fst", out), &lex_fst);
    let (idx, data) = values.finish();
    write(&format!("{}/lex.idx", out), &idx);
    write(&format!("{}/lex.blk", out), &data);
    let (idx, data) = lists.finish();
    write(&format!("{}/lists.idx", out), &idx);
    write(&format!("{}/lists.blk", out), &data);
    let mut tb = Vec::new();
    for t in &tagsets {
        tb.extend_from_slice(&t.to_le_bytes());
    }
    write(&format!("{}/tagsets.bin", out), &tb);
    let mut tw = TableWriter::new();
    for (code, n) in &langs {
        let mut b = Vec::new();
        put_str(&mut b, code);
        put_varint(&mut b, *n as u64);
        tw.push(&b);
    }
    write(&format!("{}/langs.bin", out), &tw.finish());
}
