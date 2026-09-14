#!/usr/bin/env python3
"""Pass 1: reduce the raw wiktextract dump (one JSON object per line, ~25GB) to the
fields needed for building a multilingual "any language -> English lemma" dictionary.

Output: reduced.jsonl with compact records:
  {"l": lang_code, "w": word, "p": pos,
   "g": [gloss strings (first gloss line of each sense)],
   "lk": [linked head words from senses],
   "fo": [lemma words this entry is a form of],
   "ft": [tags of form-of senses],
   "st": [sense tags],
   "f": [[form, [tags]]...],            # inflection table entries
   "tr": [[lang_code, word, sense]...], # English entries only
   "sy": [synonym words],               # English entries only
   "hy": [hypernym words]}              # English entries only
"""
import os
import sys
import orjson
from multiprocessing import Pool

SKIP_FORM_TAGS = {'inflection-template', 'table-tags', 'class', 'romanization', 'canonical'}
MAX_FORMS = 400


def process_range(args):
    path, start, end, out_path = args
    n = 0
    with open(path, 'rb') as f, open(out_path, 'wb') as out:
        f.seek(start)
        if start > 0:
            f.readline()  # skip partial line
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
            w = d.get('word')
            lc = d.get('lang_code')
            if not w or not lc:
                continue
            rec = {'l': lc, 'w': w, 'p': d.get('pos', '')}
            glosses, links, form_of, ftags, stags = [], [], [], [], []
            for s in d.get('senses', []) or []:
                gl = s.get('glosses') or s.get('raw_glosses')
                if gl:
                    glosses.append(gl[-1][:200])
                for lk in s.get('links', []) or []:
                    if lk and lk[0]:
                        links.append(lk[0])
                tags = s.get('tags') or []
                fos = s.get('form_of') or s.get('alt_of') or []
                for fo in fos:
                    fw = fo.get('word') if isinstance(fo, dict) else None
                    if fw:
                        form_of.append(fw)
                        ftags.append(tags)
                if tags:
                    stags.append(tags)
            rec['g'] = glosses
            if links:
                rec['lk'] = list(dict.fromkeys(links))[:20]
            if form_of:
                rec['fo'] = form_of
                rec['ft'] = ftags
            if stags:
                rec['st'] = stags[:10]
            forms = []
            for fm in (d.get('forms') or [])[:MAX_FORMS]:
                form = fm.get('form')
                tags = fm.get('tags') or []
                if not form or form.startswith('no-') or any(t in SKIP_FORM_TAGS for t in tags):
                    continue
                if len(form) > 60:
                    continue
                forms.append([form, tags])
            if forms:
                rec['f'] = forms
            if lc == 'en':
                trs = []
                for t in d.get('translations', []) or []:
                    tw = t.get('word')
                    tl = t.get('lang_code') or t.get('code')
                    if tw and tl:
                        trs.append([tl, tw, (t.get('sense') or '')[:120]])
                if trs:
                    rec['tr'] = trs
                sy = [x.get('word') for x in (d.get('synonyms') or []) if x.get('word')]
                for s in d.get('senses', []) or []:
                    sy += [x.get('word') for x in (s.get('synonyms') or []) if x.get('word')]
                if sy:
                    rec['sy'] = list(dict.fromkeys(sy))[:40]
                hy = [x.get('word') for x in (d.get('hypernyms') or []) if x.get('word')]
                for s in d.get('senses', []) or []:
                    hy += [x.get('word') for x in (s.get('hypernyms') or []) if x.get('word')]
                if hy:
                    rec['hy'] = list(dict.fromkeys(hy))[:20]
            out.write(orjson.dumps(rec))
            out.write(b'\n')
            n += 1
    return n


def main():
    path = sys.argv[1]
    out = sys.argv[2]
    workers = int(sys.argv[3]) if len(sys.argv) > 3 else os.cpu_count()
    size = os.path.getsize(path)
    chunk = size // workers + 1
    tmp = [f'{out}.part{i}' for i in range(workers)]
    tasks = [(path, i * chunk, min(size, (i + 1) * chunk), tmp[i]) for i in range(workers)]
    with Pool(workers) as p:
        counts = p.map(process_range, tasks)
    print('records:', sum(counts))
    with open(out, 'wb') as o:
        for t in tmp:
            with open(t, 'rb') as f:
                while True:
                    b = f.read(1 << 24)
                    if not b:
                        break
                    o.write(b)
            os.unlink(t)


if __name__ == '__main__':
    main()
