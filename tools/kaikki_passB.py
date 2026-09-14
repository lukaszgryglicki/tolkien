#!/usr/bin/env python3
"""Pass B: build the extended English key set K+ from en.jsonl.

Outputs en_links.tsv with lines:  source_key \t kind \t target_key \t tags(comma) \t weight
  kind = form   : source is an inflected form of target (tags describe the inflection)
  kind = alias  : source is a synonym / alternative spelling / hyponym of target
                  (tags = part of speech of the source page: n, vb, adj, adv)
Direct keys (Eldamo glosses, function words) are implicit; K+ = direct ∪ sources.
An optional fourth argument names the key files whose lemmas need no alias links
(normally the Quenya lexicon and the function words): a lemma glossed only in Sindarin
then still gets synonym links, so it can reach a Quenya word.
"""
import os
import sys
import json
import orjson
import collections

sys.path.insert(0, os.path.dirname(__file__))
from kcommon import fold_en  # noqa

STOP_ALIAS_TARGETS = {'be', 'do', 'have', 'go', 'thing', 'person', 'one', 'someone', 'something', 'act', 'state',
                      'quality', 'make', 'get', 'put', 'set', 'take', 'give', 'the', 'a', 'an', 'of', 'to', 'in',
                      'and', 'or', 'not', 'it', 'that', 'this', 'who', 'which', 'what', 'any', 'some', 'very',
                      'more', 'most', 'like', 'as', 'so', 'such', 'used', 'form', 'word', 'name', 'term', 'type',
                      'kind', 'sort', 'part', 'way', 'time', 'place', 'group', 'number', 'small', 'large', 'other'}


def load_direct(paths):
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
                    keys.add(line.lower())
    return keys


def main():
    en_path, out_path = sys.argv[1], sys.argv[2]
    direct = load_direct(sys.argv[3].split(','))
    alias_direct = load_direct(sys.argv[4].split(',')) if len(sys.argv) > 4 else direct
    print('direct keys:', len(direct), ' alias-direct keys:', len(alias_direct))
    recs = []
    for line in open(en_path, 'rb'):
        try:
            recs.append(orjson.loads(line))
        except Exception:
            pass
    print('english records:', len(recs))
    lemma_pos = collections.defaultdict(set)
    for r in recs:
        lemma_pos[fold_en(r['w'])].add(r.get('p'))

    links = collections.defaultdict(dict)  # (src) -> {(kind,target,tags): weight}

    def add(src, kind, target, tags, weight):
        if not src or not target or src == target:
            return
        if len(src) > 40 or len(src.split()) > 3:
            return
        key = (kind, target, ','.join(sorted(tags)))
        if links[src].get(key, 0) < weight:
            links[src][key] = weight

    # 1) aliases for lemmas not in direct
    for r in recs:
        w = fold_en(r['w'])
        p = r.get('p')
        if p in ('name', 'char', 'sym', 'punct', 'syl', 'rom', 'redir', 'root', 'affix', 'suf', 'pref', 'phrase'):
            continue
        if w in alias_direct:
            continue
        if not w.replace("'", '').replace('-', '').replace(' ', '').isalpha():
            continue
        # the runtime drops synonym routes whose Elvish word has another part of speech
        # (equal (noun) → peer → Q. tihta- "to peer")
        ptag = {p} if p in ('n', 'vb', 'adj', 'adv') else set()
        fo = r.get('fo') or []
        ft = r.get('ft') or []
        for i, target in enumerate(fo):
            t = fold_en(target)
            tags = set(ft[i]) if i < len(ft) else set()
            if 'alt' in tags or 'formof' not in tags:
                if t in direct:
                    add(w, 'alias', t, ptag, 95)
        for i, s in enumerate(r.get('sy') or []):
            t = fold_en(s)
            if t in direct and t not in STOP_ALIAS_TARGETS:
                add(w, 'alias', t, ptag, max(60, 82 - 4 * i))
        for i, c in enumerate(r.get('c') or []):
            t = fold_en(c)
            if t in direct and t not in STOP_ALIAS_TARGETS and t != w:
                add(w, 'alias', t, ptag, max(45, 72 - 6 * i))
        for i, l in enumerate((r.get('lk') or [])[:3]):
            t = fold_en(l)
            if t in direct and t not in STOP_ALIAS_TARGETS and ' ' not in t:
                add(w, 'alias', t, ptag, max(35, 55 - 8 * i))
        for i, h in enumerate((r.get('hy') or [])[:3]):
            t = fold_en(h)
            if t in direct and t not in STOP_ALIAS_TARGETS and ' ' not in t:
                add(w, 'alias', t, ptag, max(30, 50 - 6 * i))
    alias_src = set(links.keys())
    print('alias sources:', len(alias_src))
    kplus_lemmas = direct | alias_src

    # 2) inflected forms of direct and alias lemmas (from forms tables and form-of entries)
    for r in recs:
        w = fold_en(r['w'])
        if w in kplus_lemmas:
            for form, tags in r.get('f') or []:
                fk = fold_en(form)
                if fk and fk != w and len(fk.split()) <= 3:
                    tg = set(tags) - {'formof'}
                    if not tg:
                        continue
                    add(fk, 'form', w, tg, 90 if w in direct else 80)
        fo = r.get('fo') or []
        ft = r.get('ft') or []
        for i, target in enumerate(fo):
            t = fold_en(target)
            tags = set(ft[i]) if i < len(ft) else set()
            if 'formof' in tags and t in kplus_lemmas and t != w:
                tg = tags - {'formof'}
                add(w, 'form', t, tg if tg else {'alt'}, 88 if t in direct else 78)
    n = 0
    with open(out_path, 'w', encoding='utf-8') as out:
        for src in sorted(links):
            items = sorted(links[src].items(), key=lambda kv: -kv[1])[:6]
            for (kind, target, tags), weight in items:
                out.write(f'{src}\t{kind}\t{target}\t{tags}\t{weight}\n')
                n += 1
    print('links written:', n, ' K+ size:', len(kplus_lemmas | set(links.keys())))
    with open(os.path.splitext(out_path)[0] + '_keys.txt', 'w', encoding='utf-8') as f:
        for k in sorted(kplus_lemmas | set(links.keys())):
            f.write(k + '\n')


if __name__ == '__main__':
    main()
