#!/usr/bin/env python3
"""Extract a usable Neo-Quenya / Neo-Sindarin lexicon from eldamo-data.xml.

Mirrors the filtering Eldamo itself uses for its "Neo-Quenya Vocabulary" list:
  * languages nq/q/mq (Neo-Sindarin: ns/s/n), not eq/g/en
  * no <deprecated>, no <see>, no deleted marks (- or |)
  * no phrases, texts, grammar, phonetics, roots
Names are kept in a separate table (used for capitalised tokens).

Output: JSON with entries, parsed English keys (scored later) and attested inflections.
"""
import json
import re
import sys
import collections
import xml.etree.ElementTree as ET

sys.path.insert(0, __import__('os').path.dirname(__file__))
from glossparse import parse_gloss  # noqa: E402

SUPERSCRIPTS = '¹²³⁴⁵⁶⁷⁸⁹'
NAME_SPEECH = {'masc-name', 'fem-name', 'place-name', 'proper-name', 'collective-name'}
SKIP_SPEECH = {'phrase', 'text', 'grammar', 'root', 'phoneme', 'phonetic-rule', 'phonetic-group', 'phonetics', '?'}

LANG_GROUPS = {
    'q': ('nq', 'q', 'mq'),
    's': ('ns', 's', 'n'),
}


def strip_markers(v: str) -> str:
    v = v.strip()
    for ch in SUPERSCRIPTS + '*#?!^†':
        v = v.replace(ch, '')
    v = v.replace('[', '').replace(']', '')
    return v.strip()


def normalize_quenya(v: str) -> str:
    """Eldamo's normalize-spelling plus a few extra cleanups."""
    v = strip_markers(v)
    v = re.sub(r'\(([^)]*)\)', r'\1', v)  # keep optional segments
    # archaic letters
    v = v.replace('þ', 's').replace('Þ', 'S').replace('ñ', 'n').replace('Ñ', 'N').replace('ŋ', 'n')
    v = v.replace('ā', 'á').replace('ē', 'é').replace('ī', 'í').replace('ō', 'ó').replace('ū', 'ú')
    v = v.replace('ä', 'a')
    # q -> qu, ks -> x, k -> c
    v = re.sub(r'q(?!u)', 'qu', v)
    v = re.sub(r'Q(?!u)', 'Qu', v)
    v = v.replace('ks', 'x').replace('k', 'c').replace('K', 'C')
    # diaereses used by Eldamo for display of separately pronounced vowels
    v = v.replace('oa', 'öa').replace('ea', 'ëa').replace('Ea', 'Ëa')
    # final e -> ë for words (not for nye/lye/tye and very short forms)
    core = v.rstrip('-')
    if core not in ('nye', 'lye', 'tye') and len(core) >= 3 and core.endswith('e'):
        v = core[:-1] + 'ë' + v[len(core):]
    return v


def normalize_sindarin(v: str) -> str:
    v = strip_markers(v)
    v = re.sub(r'\(([^)]*)\)', r'\1', v)
    v = v.replace('ā', 'â').replace('ē', 'ê').replace('ī', 'î').replace('ō', 'ô').replace('ū', 'û')
    v = v.replace('k', 'c').replace('K', 'C')
    return v


def infer_verb_class(form: str, explicit):
    if explicit:
        return explicit
    stem = form.rstrip('-')
    if not stem:
        return None
    if stem.endswith('ta'):
        return 'ta-formative'
    if stem.endswith('ya'):
        return 'ya-formative'
    if stem.endswith('na'):
        return 'na-formative'
    if stem.endswith('a'):
        return 'a-verb'
    if stem.endswith('u'):
        return 'u-verb'
    if stem[-1] in 'aeiouáéíóúë':
        return 'irregular-verb'
    return 'basic-verb'


def extract(root, group: str):
    langs = LANG_GROUPS[group]
    norm = normalize_quenya if group == 'q' else normalize_sindarin
    parent = {c: p for p in root.iter() for c in p}
    entries, names = [], []
    seen = {}
    for w in root.iter('word'):
        l = w.get('l')
        if l not in langs:
            continue
        speech = (w.get('speech') or '').strip()
        mark = w.get('mark') or ''
        if w.find('deprecated') is not None or w.find('see') is not None or w.find('combine') is not None:
            continue
        if '-' in mark or '|' in mark or '‽' in mark:
            continue
        if speech in SKIP_SPEECH or speech.startswith('phon'):
            continue
        gloss = w.get('gloss') or ''
        ngloss = w.get('ngloss')
        if gloss == '[unglossed]' and not ngloss:
            continue
        use_gloss = ngloss or gloss
        if not use_gloss:
            continue
        raw_v = w.get('v') or ''
        if not raw_v or '[' in raw_v and ']' in raw_v and re.search(r'\[[^\]]*\]', raw_v) and raw_v.startswith('['):
            continue  # phonetic descriptions like "[a]"
        form = norm(raw_v)
        if not form or ' became ' in form or len(form) > 40:
            continue
        stem = w.get('stem')
        if stem:
            stem = norm(stem.replace('*', ''))
        is_name = speech in NAME_SPEECH
        parsed = parse_gloss(use_gloss, speech)
        if not parsed:
            continue
        # attested inflections
        infl = collections.defaultdict(list)
        refs = 0
        for ref in w.findall('ref'):
            refs += 1
            for inf in ref.findall('inflect'):
                fname = inf.get('form')
                if fname:
                    fv = norm(ref.get('v') or '')
                    if fv and fv not in infl[fname]:
                        infl[fname].append(fv)
        vclass = None
        if 'vb' in speech:
            c = w.find('class')
            vclass = infer_verb_class(form, c.get('form') if c is not None else None)
        nested = parent.get(w) is not None and parent[w].tag == 'word'
        entry = {
            'form': form,
            'raw': raw_v,
            'stem': stem,
            'pos': speech,
            'lang': l,
            'mark': mark,
            'nested': nested,
            'vclass': vclass,
            'tengwar': w.get('tengwar'),
            'refs': refs,
            'gloss': use_gloss,
            'keys': [{'key': p['key'], 'pos': p['pos'], 'flags': sorted(p['flags']), 'group': p['group'], 'item': p['item']} for p in parsed],
            'infl': dict(infl),
            'page_id': w.get('page-id'),
        }
        key = (form.lower(), speech, use_gloss.lower())
        if key in seen:
            # prefer attested (q) over mq/nq duplicates
            old = entries[seen[key]] if not is_name else names[seen[key]]
            rank = {'q': 0, 's': 0, 'mq': 1, 'n': 1, 'nq': 2, 'ns': 2}
            if rank.get(l, 3) < rank.get(old['lang'], 3):
                old.update(entry)
            else:
                old['refs'] += refs
            continue
        if is_name:
            seen[key] = len(names)
            names.append(entry)
        else:
            seen[key] = len(entries)
            entries.append(entry)
    return entries, names


def main():
    xml_path = sys.argv[1] if len(sys.argv) > 1 else 'research/eldamo-data.xml'
    out_dir = sys.argv[2] if len(sys.argv) > 2 else 'research/build'
    tree = ET.parse(xml_path)
    root = tree.getroot()
    version = root.get('version')
    for group in ('q', 's'):
        entries, names = extract(root, group)
        for i, e in enumerate(entries):
            e['id'] = i
        for i, e in enumerate(names):
            e['id'] = i
        out = {'version': version, 'entries': entries, 'names': names}
        path = f'{out_dir}/lexicon_{group}.json'
        with open(path, 'w', encoding='utf-8') as f:
            json.dump(out, f, ensure_ascii=False, indent=0)
        pos = collections.Counter(e['pos'] for e in entries)
        print(f'{group}: {len(entries)} entries, {len(names)} names -> {path}')
        print('   ', pos.most_common(12))
        keys = set(k['key'] for e in entries for k in e['keys'])
        print('    distinct english keys:', len(keys), ' single-word:', sum(1 for k in keys if ' ' not in k))


if __name__ == '__main__':
    main()
