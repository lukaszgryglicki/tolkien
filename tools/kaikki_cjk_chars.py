#!/usr/bin/env python3
"""Supplement the pass-C shards with CJK single-character entries.

Wiktionary files Chinese (and Japanese kanji) single-character words under the
part of speech "character" whose glosses mix nouns, verbs, particles and
prepositions ("to give", "for; for the benefit of", "salary; pay"...).  Pass A
skips that POS, so the lexicon lacked exactly the words a Chinese sentence is
made of.  This tool re-reads those records, infers a POS per gloss (using the
primary POS of the English gloss words), emits lemma candidates in the pass-C
".lem" format and links simplified characters to their traditional form.

usage: kaikki_cjk_chars.py cjk.jsonl en.jsonl kplus_keys.txt shards_dir
  cjk.jsonl = lines of reduced.jsonl with "l":"zh" or "l":"ja"
Appends to shards_dir/s{N}.lem and .frm (N = crc32 shard of the language).
"""
import os
import re
import sys
import zlib
from collections import Counter, defaultdict

import orjson

sys.path.insert(0, os.path.dirname(__file__))
from kcommon import fold_key, fold_en, clean_gloss_items, CLASSIFIER_RE, PAREN_RE, POS_MAP  # noqa

NSHARDS = 64
SKIP_RE = re.compile(
    r'^(?:\(.*?\)\s*)?(?:used|classifier|indicates?|indicating|meaningless|pronominal|a light verb|only used|'
    r'short for|original form|alternative form|alt\. form|misspelling|a pronoun that|a particle|particle|placed|'
    r'marker|suffix|prefix|see |abbreviation|variant of|same as|synonym of|erroneous|nonstandard|obsolete form|'
    r'archaic form|ancient form|initialism|a surname|surname|a male given name|a female given name|'
    r'a unisex given name|given name|transliteration|transcription|phonetic|interjection|expressing|'
    r'sentence-final|final particle|modal particle|question particle|structural particle|aspect)', re.I)
PRON = {'i', 'me', 'my', 'mine', 'we', 'us', 'our', 'ours', 'you', 'your', 'yours', 'he', 'him', 'his', 'she',
        'her', 'hers', 'it', 'its', 'they', 'them', 'their', 'theirs', 'this', 'that', 'these', 'those', 'who',
        'whom', 'whose', 'what', 'which', 'oneself', 'myself', 'yourself', 'himself', 'herself', 'itself',
        'ourselves', 'themselves', 'self', 'each other', 'one another', 'somebody', 'someone', 'something',
        'anybody', 'anyone', 'anything', 'nobody', 'nothing', 'everybody', 'everyone', 'everything', 'this thing',
        'that thing', 'here', 'there', 'where', 'how', 'why', 'when', 'thou', 'thee', 'thy', 'ye'}
PREP = {'in', 'at', 'on', 'to', 'for', 'with', 'by', 'from', 'of', 'during', 'until', 'till', 'towards', 'toward',
        'than', 'about', 'into', 'onto', 'over', 'under', 'above', 'below', 'behind', 'before', 'after', 'between',
        'among', 'against', 'through', 'across', 'along', 'around', 'since', 'without', 'within', 'upon', 'near',
        'beside', 'like', 'as', 'per', 'via', 'except', 'inside', 'outside', 'beyond', 'according to', 'regarding',
        'concerning', 'because of', 'in front of', 'on top of', 'next to', 'out of', 'up to'}
CONJ = {'and', 'or', 'but', 'if', 'because', 'then', 'so', 'although', 'though', 'while', 'whereas', 'unless',
        'when', 'whether', 'nor', 'yet', 'lest', 'as soon as', 'moreover', 'furthermore', 'however', 'therefore',
        'thus', 'hence', 'also', 'even if', 'in order to', 'so that', 'as well as', 'once', 'until'}
NUM = {'zero', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine', 'ten', 'hundred',
       'thousand', 'ten thousand', 'million', 'billion', 'half', 'twenty', 'thirty', 'forty', 'fifty', 'sixty',
       'seventy', 'eighty', 'ninety', 'first', 'second', 'third', 'both', 'several', 'few', 'many', 'a few',
       'some', 'all', 'every', 'each', 'other', 'another'}
CONTENT = ('n', 'vb', 'adj', 'adv')
# modern-usage senses that gloss inference cannot recover from the classical-first entries
OVERRIDES = {
    ('zh', '是'): [('vb', 'be', 95)],
    ('zh', '有'): [('vb', 'have', 95)],
    ('zh', '沒'): [('adv', 'not', 92)],
    ('zh', '很'): [('adv', 'very', 92)],
    ('zh', '也'): [('adv', 'also', 92)],
    ('zh', '都'): [('adv', 'all', 90)],
    ('zh', '和'): [('conj', 'and', 92), ('prep', 'with', 80)],
    ('zh', '在'): [('prep', 'in', 92), ('prep', 'at', 90), ('vb', 'be', 70)],
    ('zh', '這'): [('pron', 'this', 92)],
    ('zh', '那'): [('pron', 'that', 92)],
    ('zh', '他'): [('pron', 'he', 95)],
    ('zh', '她'): [('pron', 'she', 95)],
    ('zh', '它'): [('pron', 'it', 95)],
    ('zh', '你'): [('pron', 'you', 95)],
    ('zh', '您'): [('pron', 'you', 95)],
    ('zh', '我'): [('pron', 'i', 95)],
    ('zh', '誰'): [('pron', 'who', 92)],
    ('zh', '會'): [('vb', 'can', 85), ('vb', 'will', 80)],
    ('zh', '能'): [('vb', 'can', 88)],
    ('zh', '要'): [('vb', 'want', 85), ('vb', 'will', 78)],
    ('zh', '說'): [('vb', 'say', 92)],
    ('zh', '看'): [('vb', 'see', 90), ('vb', 'look', 88)],
    ('zh', '去'): [('vb', 'go', 92)],
    ('zh', '來'): [('vb', 'come', 92)],
    ('zh', '做'): [('vb', 'do', 90), ('vb', 'make', 88)],
    ('zh', '想'): [('vb', 'think', 88), ('vb', 'want', 84)],
    ('zh', '給'): [('vb', 'give', 95), ('prep', 'to', 80)],
    ('zh', '與'): [('conj', 'and', 88), ('prep', 'with', 84), ('vb', 'give', 70)],
    ('zh', '而'): [('conj', 'and', 85), ('conj', 'but', 80)],
    ('zh', '但'): [('conj', 'but', 92)],
    ('zh', '因'): [('conj', 'because', 80)],
    ('zh', '為'): [('prep', 'for', 85), ('vb', 'be', 60)],
    ('zh', '從'): [('prep', 'from', 92)],
    ('zh', '到'): [('prep', 'to', 88), ('vb', 'arrive', 80)],
    ('zh', '向'): [('prep', 'towards', 90)],
    ('zh', '上'): [('prep', 'on', 85), ('n', 'top', 75), ('vb', 'go up', 60)],
    ('zh', '下'): [('prep', 'under', 85), ('n', 'bottom', 70), ('vb', 'descend', 60)],
    ('zh', '裏'): [('prep', 'inside', 85)],
    ('zh', '裡'): [('prep', 'inside', 85)],
    ('zh', '中'): [('prep', 'in', 80), ('n', 'middle', 75)],
    ('zh', '大'): [('adj', 'big', 92), ('adj', 'great', 88)],
    ('zh', '小'): [('adj', 'small', 92)],
    ('zh', '好'): [('adj', 'good', 92)],
    ('zh', '新'): [('adj', 'new', 92)],
    ('zh', '老'): [('adj', 'old', 92)],
    ('zh', '人'): [('n', 'person', 92), ('n', 'man', 85)],
    ('zh', '山'): [('n', 'mountain', 92)],
    ('zh', '水'): [('n', 'water', 92)],
    ('zh', '火'): [('n', 'fire', 92)],
    ('zh', '天'): [('n', 'sky', 90), ('n', 'day', 85), ('n', 'heaven', 80)],
    ('zh', '日'): [('n', 'sun', 88), ('n', 'day', 86)],
    ('zh', '月'): [('n', 'moon', 88), ('n', 'month', 85)],
    ('zh', '星'): [('n', 'star', 92)],
    ('zh', '樹'): [('n', 'tree', 92)],
    ('zh', '海'): [('n', 'sea', 92)],
    ('zh', '光'): [('n', 'light', 92)],
    ('zh', '王'): [('n', 'king', 95)],
    ('zh', '劍'): [('n', 'sword', 95)],
    ('zh', '書'): [('n', 'book', 92), ('vb', 'write', 70)],
    ('zh', '寫'): [('vb', 'write', 92)],
    ('zh', '讀'): [('vb', 'read', 92)],
    ('zh', '走'): [('vb', 'walk', 88), ('vb', 'go', 80)],
    ('zh', '吃'): [('vb', 'eat', 92)],
    ('zh', '喝'): [('vb', 'drink', 92)],
    ('zh', '愛'): [('vb', 'love', 92), ('n', 'love', 85)],
    ('zh', '死'): [('vb', 'die', 92)],
    ('zh', '生'): [('vb', 'live', 80), ('vb', 'be born', 78), ('n', 'life', 75)],
    ('zh', '住'): [('vb', 'live', 88), ('vb', 'dwell', 86)],
    ('zh', '唱'): [('vb', 'sing', 92)],
    ('zh', '歌'): [('n', 'song', 92), ('vb', 'sing', 80)],
    ('zh', '見'): [('vb', 'see', 92)],
    ('zh', '聽'): [('vb', 'hear', 90), ('vb', 'listen', 88)],
    ('zh', '知'): [('vb', 'know', 92)],
    ('zh', '找'): [('vb', 'seek', 88), ('vb', 'find', 84)],
    ('zh', '拿'): [('vb', 'take', 90)],
    ('zh', '用'): [('vb', 'use', 90), ('prep', 'with', 80)],
    ('zh', '被'): [('prep', 'by', 88)],
    ('zh', '讓'): [('vb', 'let', 88)],
    ('zh', '請'): [('vb', 'please', 80), ('vb', 'ask', 78)],
    ('zh', '請'): [('vb', 'ask', 80)],
    ('zh', '快'): [('adj', 'fast', 90), ('adv', 'quickly', 85)],
    ('zh', '慢'): [('adj', 'slow', 90)],
    ('zh', '多'): [('adj', 'many', 90), ('adj', 'much', 86)],
    ('zh', '少'): [('adj', 'few', 90), ('adj', 'little', 84)],
    ('zh', '高'): [('adj', 'tall', 90), ('adj', 'high', 88)],
    ('zh', '長'): [('adj', 'long', 90)],
    ('zh', '白'): [('adj', 'white', 92)],
    ('zh', '黑'): [('adj', 'black', 92)],
    ('zh', '紅'): [('adj', 'red', 92)],
    ('zh', '金'): [('n', 'gold', 92)],
    ('zh', '銀'): [('n', 'silver', 92)],
    ('zh', '鐵'): [('n', 'iron', 92)],
    ('zh', '石'): [('n', 'stone', 92)],
    ('zh', '花'): [('n', 'flower', 92)],
    ('zh', '鳥'): [('n', 'bird', 92)],
    ('zh', '馬'): [('n', 'horse', 92)],
    ('zh', '龍'): [('n', 'dragon', 92)],
    ('zh', '門'): [('n', 'door', 90), ('n', 'gate', 88)],
    ('zh', '家'): [('n', 'home', 90), ('n', 'house', 88), ('n', 'family', 85)],
    ('zh', '國'): [('n', 'country', 90), ('n', 'kingdom', 88)],
    ('zh', '城'): [('n', 'city', 90)],
    ('zh', '路'): [('n', 'road', 92)],
    ('zh', '手'): [('n', 'hand', 92)],
    ('zh', '心'): [('n', 'heart', 92)],
    ('zh', '眼'): [('n', 'eye', 92)],
    ('zh', '名'): [('n', 'name', 92)],
    ('zh', '話'): [('n', 'word', 85), ('n', 'speech', 82)],
    ('zh', '年'): [('n', 'year', 92)],
    ('zh', '夜'): [('n', 'night', 92)],
    ('zh', '風'): [('n', 'wind', 92)],
    ('zh', '雨'): [('n', 'rain', 92)],
    ('zh', '雪'): [('n', 'snow', 92)],
    ('zh', '雲'): [('n', 'cloud', 92)],
    ('zh', '友'): [('n', 'friend', 92)],
    ('zh', '父'): [('n', 'father', 92)],
    ('zh', '母'): [('n', 'mother', 92)],
    ('zh', '子'): [('n', 'child', 85), ('n', 'son', 84)],
    ('zh', '女'): [('n', 'woman', 90), ('n', 'daughter', 84)],
    ('zh', '男'): [('n', 'man', 90)],
    ('zh', '神'): [('n', 'god', 92)],
    ('zh', '世'): [('n', 'world', 88)],
    ('zh', '界'): [('n', 'realm', 85)],
}


def shard_of(lang: str) -> int:
    return zlib.crc32(lang.encode()) % NSHARDS


def load_en_pos(path):
    """Primary (first listed) POS of every English lemma."""
    prim = {}
    with open(path, 'rb') as f:
        for line in f:
            try:
                d = orjson.loads(line)
            except Exception:
                continue
            p = d.get('p', '')
            if p not in ('n', 'vb', 'adj', 'adv', 'pron', 'prep', 'conj', 'num', 'det', 'part', 'interj'):
                continue
            w = fold_en(d['w'])
            if w and w not in prim:
                prim[w] = p
    return prim


def infer_pos(gloss: str, items, en_pos):
    g = CLASSIFIER_RE.sub('', gloss)
    for _ in range(2):
        g = PAREN_RE.sub('', g)
    g = g.strip().lower()
    if not g or SKIP_RE.match(g):
        return None
    if not items:
        return None
    if re.match(r'^(?:to|be|to be|being)\s', g):
        return 'vb'
    low = [it.lower() for it in items]
    if all(it in PRON for it in low):
        return 'pron'
    if all(it in PREP for it in low):
        return 'prep'
    if all(it in CONJ for it in low):
        return 'conj'
    if all(it in NUM for it in low):
        return 'num'
    votes = Counter()
    for it in low:
        p = en_pos.get(it)
        if p is None and ' ' in it:
            p = en_pos.get(it.split()[-1])
        if p in CONTENT or p in ('pron', 'prep', 'conj', 'num', 'det', 'part', 'interj'):
            votes[p] += 1
    if not votes:
        return 'n'
    order = {'n': 0, 'adj': 1, 'vb': 2, 'adv': 3, 'pron': 4, 'prep': 5, 'conj': 6, 'num': 7, 'det': 8, 'part': 9,
             'interj': 10}
    best = sorted(votes.items(), key=lambda kv: (-kv[1], order.get(kv[0], 99)))[0][0]
    if best == 'vb':
        # bare "supply; provide" glosses of Chinese characters are nouns unless the gloss says "to"
        return 'n' if votes.get('n') else 'vb'
    return best


def main():
    cjk_path, en_path, keys_path, shards = sys.argv[1:5]
    kplus = set(l.rstrip('\n') for l in open(keys_path, encoding='utf-8'))
    en_pos = load_en_pos(en_path)
    print('english lemmas with POS:', len(en_pos))
    trad2simp = defaultdict(Counter)
    records = []  # (lang, word, record)
    with open(cjk_path, 'rb') as f:
        for line in f:
            try:
                d = orjson.loads(line)
            except Exception:
                continue
            lang, w = d['l'], d['w']
            if lang == 'zh':
                for form, tags in d.get('f') or []:
                    if 'Simplified-Chinese' in tags and len(form) == len(w) and form != w:
                        for a, b in zip(w, form):
                            if a != b:
                                trad2simp[a][b] += 1
            if d.get('p') == 'character' and len(w) == 1 and lang in ('zh', 'ja'):
                records.append((lang, w, d))
    simp_of = {t: c.most_common(1)[0][0] for t, c in trad2simp.items()}
    print('trad->simp pairs:', len(simp_of), ' character records:', len(records))

    out_lem = defaultdict(list)
    out_frm = defaultdict(list)
    seen_rec = Counter()
    n_lem = n_frm = 0
    for lang, w, d in records:
        fw = fold_key(lang, w)
        if not fw:
            continue
        k = seen_rec[(lang, fw)]
        seen_rec[(lang, fw)] += 1
        base_pen = 4 * k + (6 if lang == 'ja' else 0)
        by_pos = defaultdict(dict)
        for si, g in enumerate(d.get('g') or []):
            if si >= 10:
                break
            items = clean_gloss_items(g, 4)
            pos = infer_pos(g, items, en_pos)
            if not pos:
                continue
            for ii, item in enumerate(items):
                it = fold_en(item)
                if it in kplus:
                    wgt = max(25, 85 - 10 * si - 4 * ii - base_pen)
                elif ' ' in it and pos == 'n' and it.split()[-1] in kplus and len(it.split()[-1]) > 2:
                    it = it.split()[-1]
                    wgt = max(15, 50 - 10 * si - 4 * ii - base_pen)
                else:
                    continue
                if by_pos[pos].get(it, 0) < wgt:
                    by_pos[pos][it] = wgt
        if k == 0:
            for pos, e, wgt in OVERRIDES.get((lang, w), ()):
                if e in kplus and by_pos[pos].get(e, 0) < wgt:
                    by_pos[pos][e] = wgt
        s = shard_of(lang)
        for pos, cands in by_pos.items():
            top = sorted(cands.items(), key=lambda kv: -kv[1])[:8]
            out_lem[s].append(f"{lang}\t{fw}\t{pos}\t{';'.join(f'{e}|{wg}' for e, wg in top)}\n")
            n_lem += 1
        for i, lemma in enumerate(d.get('fo') or []):
            tags = (d.get('ft') or [[]])[i] if i < len(d.get('ft') or []) else []
            if 'alt-of' in tags and len(lemma) == 1:
                fl = fold_key(lang, lemma)
                if fl and fl != fw:
                    out_frm[s].append(f"{lang}\t{fw}\t{fl}\talt\n")
                    n_frm += 1
        if lang == 'zh' and by_pos:
            simp = simp_of.get(w)
            if simp and simp != w:
                out_frm[s].append(f"zh\t{fold_key('zh', simp)}\t{fw}\talt\n")
                n_frm += 1
    for s, lines in out_lem.items():
        with open(f'{shards}/s{s}.lem', 'a', encoding='utf-8') as o:
            o.writelines(lines)
    for s, lines in out_frm.items():
        with open(f'{shards}/s{s}.frm', 'a', encoding='utf-8') as o:
            o.writelines(lines)
    print('lemma lines:', n_lem, ' form lines:', n_frm)


if __name__ == '__main__':
    main()
