#!/usr/bin/env python3
"""Pass C: filter foreign records by K+ and shard them by language.

Per shard s (hash of lang code), writes:
  shard_s.lem : lang \t form \t pos \t E|w;E|w...      (lemma candidates)
  shard_s.frm : lang \t form \t lemma \t tags           (inflected form of lemma)
Translation pairs (tr.jsonl) are also sharded into shard_s.tr : lang \t form \t E \t pos
"""
import os
import sys
import orjson
import zlib
from multiprocessing import Pool

sys.path.insert(0, os.path.dirname(__file__))
from kcommon import fold_key, fold_en  # noqa

NSHARDS = 64


def shard_of(lang: str) -> int:
    return zlib.crc32(lang.encode()) % NSHARDS


def process_range(args):
    path, start, end, out_dir, wid, keys_path = args
    kplus = set(l.rstrip('\n') for l in open(keys_path, encoding='utf-8'))
    lem = [open(f'{out_dir}/w{wid}_s{s}.lem', 'w', encoding='utf-8') for s in range(NSHARDS)]
    frm = [open(f'{out_dir}/w{wid}_s{s}.frm', 'w', encoding='utf-8') for s in range(NSHARDS)]
    n_lem = n_frm = 0
    with open(path, 'rb') as f:
        f.seek(start)
        if start > 0:
            f.readline()
        pos = f.tell()
        while pos < end:
            line = f.readline()
            if not line:
                break
            pos += len(line)
            try:
                r = orjson.loads(line)
            except Exception:
                continue
            lang, w, p = r['l'], r['w'], r.get('p', '')
            if '\t' in w or '\x01' in w or '\n' in w:
                continue
            fw = fold_key(lang, w)
            if not fw or len(fw) > 60 or len(fw.split()) > 3:
                continue
            s = shard_of(lang)
            cands = {}
            for si, items in enumerate(r.get('c') or []):
                for ii, item in enumerate(items):
                    it = fold_en(item)
                    if it in kplus:
                        wgt = max(25, 85 - 10 * si - 4 * ii)
                        if cands.get(it, 0) < wgt:
                            cands[it] = wgt
                    elif p == 'n' and ' ' in it:
                        head = it.split()[-1]
                        if head in kplus and len(head) > 2:
                            wgt = max(15, 50 - 10 * si - 4 * ii)
                            if cands.get(head, 0) < wgt:
                                cands[head] = wgt
            for li, l in enumerate(r.get('lk') or []):
                fl = fold_en(l)
                if fl in kplus and fl.replace(' ', '').replace('-', '').isalpha():
                    wgt = max(20, 60 - 6 * li)
                    if cands.get(fl, 0) < wgt:
                        cands[fl] = wgt
            if cands:
                top = sorted(cands.items(), key=lambda kv: -kv[1])[:8]
                lem[s].write(f"{lang}\t{fw}\t{p}\t{';'.join(f'{e}|{wg}' for e, wg in top)}\n")
                n_lem += 1
            fo = r.get('fo') or []
            ft = r.get('ft') or []
            for i, lemma in enumerate(fo):
                fl = fold_key(lang, lemma)
                if fl and fl != fw and '\t' not in fl:
                    tags = ft[i] if i < len(ft) else []
                    tags = [t for t in tags if t != 'formof']
                    frm[s].write(f"{lang}\t{fw}\t{fl}\t{','.join(tags)}\n")
                    n_frm += 1
            for form, tags in r.get('f') or []:
                ff = fold_key(lang, form)
                if ff and ff != fw and '\t' not in ff and len(ff) <= 60 and len(ff.split()) <= 3:
                    frm[s].write(f"{lang}\t{ff}\t{fw}\t{','.join(tags)}\n")
                    n_frm += 1
    for fh in lem + frm:
        fh.close()
    return n_lem, n_frm


def shard_translations(tr_path, out_dir):
    outs = [open(f'{out_dir}/s{s}.tr', 'w', encoding='utf-8') for s in range(NSHARDS)]
    n = 0
    for line in open(tr_path, 'rb'):
        try:
            r = orjson.loads(line)
        except Exception:
            continue
        lang, w = r['l'], r['w']
        if '\t' in w or '\x01' in w:
            continue
        fw = fold_key(lang, w)
        if not fw or len(fw) > 60:
            continue
        outs[shard_of(lang)].write(f"{lang}\t{fw}\t{r['e']}\t{r['p']}\n")
        n += 1
    for o in outs:
        o.close()
    return n


def main():
    fo_path, tr_path, keys_path, out_dir = sys.argv[1:5]
    workers = int(sys.argv[5]) if len(sys.argv) > 5 else os.cpu_count()
    os.makedirs(out_dir, exist_ok=True)
    size = os.path.getsize(fo_path)
    chunk = size // workers + 1
    tasks = [(fo_path, i * chunk, min(size, (i + 1) * chunk), out_dir, i, keys_path) for i in range(workers)]
    with Pool(workers) as p:
        counts = p.map(process_range, tasks)
    print('lemma records:', sum(c[0] for c in counts), 'form records:', sum(c[1] for c in counts))
    # concatenate worker files per shard
    for s in range(NSHARDS):
        for ext in ('lem', 'frm'):
            with open(f'{out_dir}/s{s}.{ext}', 'wb') as o:
                for i in range(workers):
                    t = f'{out_dir}/w{i}_s{s}.{ext}'
                    with open(t, 'rb') as f:
                        while True:
                            b = f.read(1 << 24)
                            if not b:
                                break
                            o.write(b)
                    os.unlink(t)
    print('translation pairs:', shard_translations(tr_path, out_dir))


if __name__ == '__main__':
    main()
