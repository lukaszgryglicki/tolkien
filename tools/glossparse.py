"""Parsing of Eldamo-style English glosses into lookup keys.

Eldamo gloss notation (see eldamo.org "Terminology and Notations"):
  ;            separates distinct senses
  ,            separates near-synonyms within a sense
  *            unattested / editorial gloss
  †            archaic or poetic
  ?            uncertain
  ⚠️           questionable (Eldamo's vocabulary list cuts everything after it)
  [ᴹQ.] [ᴱQ.]  gloss taken from an earlier conceptual period
  (lit.) ...   literal meaning (not a usable translation)
  ( ... )      optional words or qualifiers
  [ ... ]      editorial clarifications
"""
import re

PERIOD_RE = re.compile(r'\[(ᴱQ\.|ᴹQ\.|ᴺQ\.|Q\.|S\.|N\.|ᴱN\.|G\.|ON\.|ᴺS\.|ᴱQ\., ᴹQ\.|ᴹQ\., ᴱQ\.)\]')

QUALIFIER_START = (
    'of ', 'as ', 'in ', 'for ', 'with ', 'by ', 'to ', 'e.g.', 'i.e.', 'esp', 'orig.', 'orig ', 'f.', 'm.', 'dual',
    'intr', 'trans', 'tr.', 'mentioned', 'plural', 'emphatic', 'polite', 'inclusive', 'exclusive',
    'familiar', 'colloquial', 'informal', 'intimate', 'impersonal', 'relative', 'abstract', 'physical',
    'diminutive', 'single', 'something', 'someone', 'oneself', 'future', 'past', 'present', 'aorist',
    'anaphoric', 'not ', 'from ', 'at ', 'on ', 'about ', 'against ', 'usually', 'often', 'especially',
    'generally', 'also', 'only', 'chiefly', 'said of', 'used ', 'when ', 'if ', 'or ', 'and ', 'etc',
    'sing.', 'pl.', 'adj.', 'n.', 'vb.', 'poetic', 'archaic', 'rare', 'figurative', 'literal', 'lit.',
    'cf.', 'compare', 'see ', '?', '=', 'both ', 'like ', 'cause to', 'act of', 'state of', 'quality of',
    'grammar', 'math', 'phys', 'anat', 'astr', 'bot', 'zool', 'geog', 'relig', 'myth',
)

ARTICLES = ('a ', 'an ', 'the ')


def _is_qualifier(content: str) -> bool:
    c = content.strip().lower()
    if not c:
        return True
    if c.startswith(QUALIFIER_START):
        return True
    if len(c.split()) >= 4:
        return True
    return False


def _split_outside_parens(s: str, sep: str):
    out, depth, cur = [], 0, []
    for ch in s:
        if ch in '([':
            depth += 1
        elif ch in ')]':
            depth = max(0, depth - 1)
        if ch == sep and depth == 0:
            out.append(''.join(cur))
            cur = []
        else:
            cur.append(ch)
    out.append(''.join(cur))
    return out


def _expand_parens(item: str):
    """Expand optional parentheticals/brackets into variants (bounded)."""
    variants = [item]
    for _ in range(6):
        new = []
        changed = False
        for v in variants:
            m = re.search(r'\(([^()]*)\)|\[([^\[\]]*)\]', v)
            if not m:
                new.append(v)
                continue
            changed = True
            content = m.group(1) if m.group(1) is not None else m.group(2)
            before, after = v[:m.start()], v[m.end():]
            cl = content.strip().lower()
            if cl in ('lit.', 'lit', 'literally', 'orig.', 'orig', 'originally'):
                # "(lit.) X" gives a literal meaning, not a usable translation
                continue
            if m.group(1) is not None and cl in ('s', 'es', 'ing', 'ed', 'd', 'er', 'en', 'ly', 'ness', 'n') \
                    and before and not before.endswith(' '):
                new.append(before + after)
                new.append(before + content + after)
                continue
            if m.group(1) is not None and _is_qualifier(content):
                new.append(before + after)
                continue
            new.append(before + after)
            new.append(before + ' ' + content.strip() + ' ' + after)
        variants = new[:16]
        if not changed:
            break
    return variants


def _clean(s: str) -> str:
    s = s.replace('’', "'").replace('‘', "'").replace('“', '').replace('”', '')
    s = re.sub(r'\s+', ' ', s).strip(' .,;:!?"\'-–—')
    return s.lower()


def parse_gloss(gloss: str, speech: str):
    """Return list of dicts: {key, pos, flags(set), group, item}."""
    if not gloss:
        return []
    results = []
    questionable = False
    text = gloss
    groups = _split_outside_parens(text, ';')
    for gi, group in enumerate(groups):
        items = _split_outside_parens(group, ',')
        periods = set()
        for ii, item in enumerate(items):
            flags = set()
            found = PERIOD_RE.findall(item)
            if found:
                periods = set(found)
                item = PERIOD_RE.sub('', item)
            if '⚠️' in item or '⚠' in item:
                questionable = True
                item = item.replace('⚠️', '').replace('⚠', '')
            if questionable:
                flags.add('warn')
            item = item.strip()
            while item and item[0] in '*†?!#':
                flags.add({'*': 'unattested', '†': 'archaic', '?': 'uncertain', '!': 'neo', '#': 'derived'}[item[0]])
                item = item[1:].strip()
            if not item:
                continue
            for p in periods:
                flags.add('period:' + p)
            for variant in _expand_parens(item):
                v = _clean(variant)
                if not v:
                    continue
                pos = speech
                if v.startswith('to ') and len(v) > 3:
                    v = v[3:]
                    if 'vb' in (speech or ''):
                        pos = 'vb'
                for a in ARTICLES:
                    if v.startswith(a) and len(v) > len(a):
                        v = v[len(a):]
                        break
                v = _clean(v)
                if not v or len(v) > 60:
                    continue
                if '/' in v:
                    parts = [p.strip() for p in re.split(r'\s*/\s*', v) if p.strip()]
                    if all(' ' not in p for p in parts):
                        for p in parts:
                            results.append({'key': p, 'pos': pos, 'flags': set(flags), 'group': gi, 'item': ii})
                        continue
                    # "word1/word2 rest": take first alternative
                    v2 = _clean(re.sub(r'/\s*\S+', '', v))
                    if v2:
                        results.append({'key': v2, 'pos': pos, 'flags': set(flags), 'group': gi, 'item': ii})
                    continue
                results.append({'key': v, 'pos': pos, 'flags': set(flags), 'group': gi, 'item': ii})
    seen = {}
    for r in results:
        k = (r['key'], r['pos'])
        if k not in seen or (r['group'], r['item']) < (seen[k]['group'], seen[k]['item']):
            seen[k] = r
    return list(seen.values())


if __name__ == '__main__':
    import sys
    for g in sys.argv[1:] or [
        'to say, speak, tell, ⚠️[ᴹQ.] mention, [ᴱQ.] talk',
        'man, male person (of any kindred: elf, human or dwarf)',
        '(wooden) chest',
        'afar, [ᴹQ.] far (off), far away',
        'sausage, (lit.) compressed meat',
        '*to sing, make music',
        'he/she, it',
        'star(s)',
        '[the] Sun',
        'lowlying, low; deep, [ᴱQ.] profound; ⚠️dark, hidden',
    ]:
        print(g)
        for r in sorted(parse_gloss(g, 'vb' if g.startswith('to') or g.startswith('*to') else 'n'), key=lambda r: (r['group'], r['item'])):
            print('   ', r['key'], '|', r['pos'], '|', sorted(r['flags']), r['group'], r['item'])
