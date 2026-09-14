#!/usr/bin/env python3
"""Pass D: join lemma candidates, inflected forms and translations per shard.

Input (per shard, from pass C): s{N}.lem, s{N}.frm, s{N}.tr
Output: research/build/lex.tsv (sorted by key), lines:
  form \x01 lang \t E|weight|pos|tagset;tagset... \t E2|... ...
where tagset = comma separated canonical tags ('' = base form).

Per-language caps keep the embedded FST small; forms are prioritised by the
weight of their best candidate, penalising rare/complex inflections.
"""
import os
import sys
from collections import defaultdict
from multiprocessing import Pool

sys.path.insert(0, os.path.dirname(__file__))
from kcommon import fold_en  # noqa

NSHARDS = 64
CAP_MIN = 8_000
CAP_MAX = 350_000
CAP_RATIO = 3
MAX_CANDS = 8
MAX_TAGSETS = 3
TR_BONUS = 15
RARE_TAGS = {'poss'}
PERSON_TAGS = {'p1', 'p2', 'p3'}


def tag_penalty(tags, pos):
    t = tags.split(',') if tags else []
    pen = 2 * len(t)
    if RARE_TAGS & set(t):
        pen += 25
    if PERSON_TAGS & set(t) and pos != 'vb':
        pen += 8
    return pen


def process_shard(args):
    in_dir, out_dir, s, caps = args
    lem = {}  # (lang, form) -> {E: (w, pos)}
    with open(f'{in_dir}/s{s}.lem', encoding='utf-8') as f:
        for line in f:
            lang, form, pos, cands = line.rstrip('\n').split('\t')
            d = lem.setdefault((lang, form), {})
            for c in cands.split(';'):
                e, w = c.rsplit('|', 1)
                w = int(w)
                old = d.get(e)
                # each English candidate keeps the POS of the record that ranks it best
                if old is None or old[0] < w or (old[0] == w and not old[1] and pos):
                    d[e] = (w, pos)
    tr = defaultdict(dict)  # (lang, form) -> {E: pos}
    with open(f'{in_dir}/s{s}.tr', encoding='utf-8') as f:
        for line in f:
            lang, form, e, pos = line.rstrip('\n').split('\t')
            tr[(lang, form)][e] = pos
    l1 = set(lem) | set(tr)
    # forms of L1 lemmas
    frm = defaultdict(set)  # (lang, form) -> {(lemma, tags)}
    with open(f'{in_dir}/s{s}.frm', encoding='utf-8') as f:
        for line in f:
            lang, form, lemma, tags = line.rstrip('\n').split('\t')
            if (lang, lemma) in l1:
                frm[(lang, form)].add((lemma, tags))
    l2 = set(frm)
    frm2 = defaultdict(set)  # forms of L2 forms (2nd hop)
    with open(f'{in_dir}/s{s}.frm', encoding='utf-8') as f:
        for line in f:
            lang, form, lemma, tags = line.rstrip('\n').split('\t')
            k = (lang, lemma)
            if k in l2 and k not in l1 and (lang, form) not in l1:
                frm2[(lang, form)].add((lemma, tags))

    def own_cands(key):
        """Candidates of key itself: {E: (w, pos, from_tr)}"""
        out = {}
        if key in lem:
            for e, (w, pos) in lem[key].items():
                out[e] = (w, pos, False)
        if key in tr:
            for e, pos in tr[key].items():
                # translation-table links are unordered (90); a link that is also an
                # early gloss of the page keeps its gloss rank as a bonus (up to 94)
                w0 = out.get(e, (0,))[0]
                if w0 < 90:
                    bonus = max(0, min(4, (w0 - 69) // 4)) if w0 else 0
                    out[e] = (90 + bonus, pos, True)
        return out

    def merge_tags(a, b):
        if not a:
            return b
        if not b:
            return a
        return ','.join(sorted(set(a.split(',')) | set(b.split(','))))

    out_path = f'{out_dir}/s{s}.lex'
    per_lang = defaultdict(list)  # lang -> [(prio, line)]
    all_keys = set(lem) | set(tr) | set(frm) | set(frm2)
    for key in all_keys:
        lang, form = key
        cands = {}  # E -> [w, pos, {tagsets}, from_tr]

        def add(e, w, pos, tags, ftr):
            c = cands.get(e)
            if c is None:
                cands[e] = [w, pos, {tags}, ftr]
            else:
                if w > c[0]:
                    c[0] = w
                if not c[1] and pos:
                    c[1] = pos
                c[2].add(tags)
                c[3] = c[3] or ftr

        own = own_cands(key)
        links = frm.get(key, ())
        lemma_names = {fold_en(l) for l, _ in links}
        for e, (w, pos, ftr) in own.items():
            if e in lemma_names and key not in tr:
                continue  # 'genitive of X' gloss artefact
            add(e, w, pos, '', ftr)
        for lemma, tags in links:
            for e, (w, pos, ftr) in own_cands((lang, lemma)).items():
                add(e, w - 3, pos, tags, ftr)
        for lemma, tags in frm2.get(key, ()):
            for lemma2, tags2 in frm.get((lang, lemma), ()):
                for e, (w, pos, ftr) in own_cands((lang, lemma2)).items():
                    add(e, w - 6, pos, merge_tags(tags, tags2), ftr)
        if not cands:
            continue
        items = []
        prio = -999
        for e, (w, pos, tagsets, ftr) in cands.items():
            ts = sorted(tagsets, key=lambda t: (t.count(',') if t else -1, t))[:MAX_TAGSETS]
            best = max(w - tag_penalty(t, pos) for t in ts) + (TR_BONUS if ftr else 0)
            prio = max(prio, best)
            items.append((w, e, pos, ts))
        items.sort(key=lambda x: (-x[0], x[1]))
        items = items[:MAX_CANDS]
        line = f"{lang}\x01{form}\t" + '\t'.join(f"{e}|{w}|{pos}|{';'.join(ts)}" for w, e, pos, ts in items)
        per_lang[lang].append((prio, line))
    stats = {}
    with open(out_path, 'w', encoding='utf-8') as o:
        n_lem = defaultdict(int)
        for lang, _ in l1:
            n_lem[lang] += 1
        for lang, rows in per_lang.items():
            cap = caps.get(lang) or min(CAP_MAX, max(CAP_MIN, CAP_RATIO * n_lem[lang]))
            n = len(rows)
            if n > cap:
                rows.sort(key=lambda r: -r[0])
                rows = rows[:cap]
            for _, line in rows:
                o.write(line + '\n')
            stats[lang] = (n, len(rows))
    return stats


def main():
    in_dir, out_dir = sys.argv[1:3]
    workers = int(sys.argv[3]) if len(sys.argv) > 3 else 16
    caps = {}
    os.makedirs(out_dir, exist_ok=True)
    tasks = [(in_dir, out_dir, s, caps) for s in range(NSHARDS)]
    # process the biggest shards first
    tasks.sort(key=lambda t: -os.path.getsize(f'{in_dir}/s{t[2]}.frm'))
    stats = {}
    with Pool(workers) as p:
        for st in p.imap_unordered(process_shard, tasks):
            stats.update(st)
    total_in = sum(v[0] for v in stats.values())
    total_out = sum(v[1] for v in stats.values())
    print(f'languages: {len(stats)} keys before caps: {total_in} after: {total_out}')
    for lang, (n, k) in sorted(stats.items(), key=lambda kv: -kv[1][0])[:40]:
        print(f'  {lang}: {n} -> {k}')


if __name__ == '__main__':
    main()
