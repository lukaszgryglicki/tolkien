#!/usr/bin/env python3
"""Pass A over reduced.jsonl (parallel).

Emits three record streams (per worker, later concatenated):
  en.jsonl : English lemma/form records (all of them)
  tr.jsonl : translation pairs  {"l": lang, "w": foreign word, "e": english key, "p": pos}
             restricted to english keys in K0 (Eldamo keys + function words)
  fo.jsonl : compact foreign records {"l","w","p","c":[candidate english strings by sense],
             "fo":[lemmas],"ft":[[tags]],"f":[[form,[canon tags]]]}
"""
import os
import sys
import json
import orjson
from multiprocessing import Pool

sys.path.insert(0, os.path.dirname(__file__))
from kcommon import fold_key, fold_en, clean_gloss_items, canon_tags, BAD_FORM_TAGS, POS_MAP, INFLECTION_GLOSS_RE  # noqa

K0 = set()


def load_k0(paths):
    keys = set()
    for p in paths:
        if p.endswith('.json'):
            d = json.load(open(p, encoding='utf-8'))
            for e in d['entries'] + d['names']:
                for k in e['keys']:
                    keys.add(k['key'])
        else:
            for line in open(p, encoding='utf-8'):
                line = line.strip()
                if line and not line.startswith('#'):
                    keys.add(line.split('\t')[0].lower())
    return keys


def clean_translation_word(w: str):
    w = w.strip()
    # drop annotations in parentheses / after a comma
    import re
    w = re.sub(r'\([^)]*\)', '', w).strip()
    w = w.split(',')[0].strip()
    if not w or len(w) > 40 or len(w.split()) > 3:
        return None
    if w.startswith(('-', '~')) or w.endswith('-'):
        return None
    return w


def process_range(args):
    path, start, end, out_prefix, k0_paths = args
    k0 = load_k0(k0_paths)
    n_en = n_tr = n_fo = 0
    with open(path, 'rb') as f, open(out_prefix + '.en', 'wb') as oen, open(out_prefix + '.tr', 'wb') as otr, \
            open(out_prefix + '.fo', 'wb') as ofo:
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
                d = orjson.loads(line)
            except Exception:
                continue
            lc = d['l']
            w = d['w']
            p = POS_MAP.get(d.get('p', ''), d.get('p', ''))
            if lc == 'en':
                rec = {'w': w, 'p': p}
                if d.get('g'):
                    rec['g'] = d['g'][0][:120]
                    cand = []
                    for gi, g in enumerate(d['g'][:3]):
                        for it in clean_gloss_items(g, 3):
                            cand.append(it)
                    if cand:
                        rec['c'] = list(dict.fromkeys(cand))[:8]
                for k in ('lk', 'sy', 'hy'):
                    if d.get(k):
                        rec[k] = d[k][:12]
                if d.get('fo'):
                    rec['fo'] = d['fo'][:4]
                    rec['ft'] = [sorted(canon_tags(t)) for t in d['ft'][:4]]
                forms = []
                for form, tags in d.get('f', []):
                    if any(t in BAD_FORM_TAGS for t in tags):
                        continue
                    ct = canon_tags(tags)
                    if not ct:
                        continue
                    forms.append([form, sorted(ct)])
                if forms:
                    rec['f'] = forms[:60]
                oen.write(orjson.dumps(rec))
                oen.write(b'\n')
                n_en += 1
                if d.get('tr') and p in ('n', 'vb', 'adj', 'adv', 'pron', 'num', 'det', 'prep', 'conj', 'interj', 'part', 'phrase'):
                    ek = fold_en(w)
                    if ek in k0:
                        for tl, tw, sense in d['tr']:
                            cw = clean_translation_word(tw)
                            if not cw:
                                continue
                            otr.write(orjson.dumps({'l': tl, 'w': cw, 'e': ek, 'p': p}))
                            otr.write(b'\n')
                            n_tr += 1
                continue
            # foreign record
            if p in ('char', 'sym', 'punct', 'syl', 'rom', 'redir', 'root', 'affix', 'suf', 'pref'):
                continue
            rec = {'l': lc, 'w': w, 'p': p}
            cands = []
            has_fo = bool(d.get('fo'))
            for gi, g in enumerate(d.get('g', [])[:6]):
                if has_fo and INFLECTION_GLOSS_RE.match(g):
                    continue
                items = clean_gloss_items(g, 4)
                if items:
                    cands.append(items[:6])
            if cands:
                rec['c'] = cands
            if d.get('lk'):
                rec['lk'] = [x for x in d['lk'] if x.isascii()][:8] or d['lk'][:8]
            if has_fo:
                rec['fo'] = d['fo'][:4]
                rec['ft'] = [sorted(canon_tags(t)) for t in d['ft'][:4]]
            forms = []
            for form, tags in d.get('f', []):
                if any(t in BAD_FORM_TAGS for t in tags):
                    continue
                ct = canon_tags(tags)
                forms.append([form, sorted(ct)])
            if forms:
                rec['f'] = forms[:200]
            if not (cands or has_fo):
                continue
            ofo.write(orjson.dumps(rec))
            ofo.write(b'\n')
            n_fo += 1
    return (n_en, n_tr, n_fo)


def main():
    path, out_dir = sys.argv[1], sys.argv[2]
    k0_paths = sys.argv[3].split(',')
    workers = int(sys.argv[4]) if len(sys.argv) > 4 else os.cpu_count()
    size = os.path.getsize(path)
    chunk = size // workers + 1
    tasks = [(path, i * chunk, min(size, (i + 1) * chunk), f'{out_dir}/partA{i}', k0_paths) for i in range(workers)]
    with Pool(workers) as p:
        counts = p.map(process_range, tasks)
    print('en/tr/fo records:', [sum(c[i] for c in counts) for i in range(3)])
    for ext in ('en', 'tr', 'fo'):
        with open(f'{out_dir}/{ext}.jsonl', 'wb') as o:
            for i in range(workers):
                t = f'{out_dir}/partA{i}.{ext}'
                with open(t, 'rb') as f:
                    while True:
                        b = f.read(1 << 24)
                        if not b:
                            break
                        o.write(b)
                os.unlink(t)


if __name__ == '__main__':
    main()
