"""Shared helpers for the kaikki passes: key folding, gloss cleaning, tag canonicalisation."""
import re
import unicodedata

# Languages whose Wiktionary forms carry diacritics that are NOT part of normal orthography
# (macrons for vowel length, stress accents, vocalisation marks).  Keys for these are folded.
STRIP_MARKS_LANGS = {
    'la', 'grc', 'ru', 'uk', 'bg', 'be', 'sr', 'mk', 'sh', 'ar', 'fa', 'he', 'arc', 'syc', 'ur', 'ps',
    'ang', 'got', 'non', 'osx', 'goh', 'ofs', 'odt', 'sa', 'pi', 'ain', 'egy', 'akk', 'sux', 'hit',
    'xcl', 'cu', 'orv', 'sla-pro', 'ine-pro', 'gem-pro', 'prg', 'lt', 'lv',
}
# Lithuanian/Latvian: Wiktionary marks pitch accents (~, `, ´) that are not written normally,
# but Latvian macrons ARE orthographic... handled below: we only strip specific combining marks.
COMBINING_TO_STRIP = {
    '\u0300', '\u0301', '\u0302', '\u0303', '\u0304', '\u0306', '\u0308', '\u030c', '\u0311', '\u0323', '\u0331',
}
ORTHOGRAPHIC_MARK_LANGS = {'lv': {'\u0304', '\u030c'}, 'lt': {'\u030c'}}
HARAKAT = re.compile(r'[\u0610-\u061a\u064b-\u065f\u0670\u06d6-\u06ed\u0640]')
NIQQUD = re.compile(r'[\u0591-\u05c7]')
SUPERSCRIPT_DIGITS = str.maketrans('', '', '¹²³⁴⁵⁶⁷⁸⁹⁰')


def fold_key(lang: str, word: str) -> str:
    w = word.strip().replace('’', "'").replace('ʼ', "'").replace('`', "'")
    w = w.translate(SUPERSCRIPT_DIGITS)
    w = unicodedata.normalize('NFC', w)
    w = w.lower()
    if lang in STRIP_MARKS_LANGS:
        keep = ORTHOGRAPHIC_MARK_LANGS.get(lang, set())
        d = unicodedata.normalize('NFD', w)
        d = ''.join(ch for ch in d if not (ch in COMBINING_TO_STRIP and ch not in keep))
        w = unicodedata.normalize('NFC', d)
        if lang in ('ar', 'fa', 'ur', 'ps'):
            w = HARAKAT.sub('', w)
        if lang in ('he', 'arc', 'syc'):
            w = NIQQUD.sub('', w)
    else:
        # universal: strip stress/length marks that never form part of orthographies when
        # they appear as *combining* characters after NFC (i.e. no precomposed letter exists)
        w = ''.join(ch for ch in w if not (unicodedata.category(ch) == 'Mn' and ch in ('\u0301', '\u0300', '\u0304') and lang not in ('vi',)))
    w = re.sub(r'\s+', ' ', w).strip()
    return w


def fold_en(word: str) -> str:
    w = word.strip().replace('’', "'")
    w = unicodedata.normalize('NFC', w).lower()
    w = re.sub(r'\s+', ' ', w).strip(' .,;:!?"')
    return w


CLASSIFIER_RE = re.compile(r'\((?:Classifier|classifier)[^)]*\)')
PAREN_RE = re.compile(r'\([^()]*\)')
BRACKET_RE = re.compile(r'\[[^\[\]]*\]')
INFLECTION_GLOSS_RE = re.compile(
    r'^(?:[a-z\- ]+ )?(?:form|plural|singular|genitive|dative|accusative|nominative|vocative|ablative|locative|instrumental|'
    r'participle|infinitive|gerund|imperative|indicative|subjunctive|conditional|present|past|future|perfect|imperfect|'
    r'preterite|comparative|superlative|feminine|masculine|neuter|diminutive|augmentative|romanization|alternative|'
    r'abbreviation|initialism|acronym|misspelling|obsolete|archaic|dated|synonym|alternative spelling|'
    r'alternative form|alternative letter-case|inflection|conjugation|declension|nonstandard|rare|informal|'
    r'clipping|contraction|ellipsis|eye dialect|pronunciation spelling|reflexive|passive|active|first-person|'
    r'second-person|third-person|definite|indefinite|short|long|soft|hard|construct|dual|collective|singulative|'
    r'attributive|predicative|stem|root|combining|hanja|kanji|hiragana|katakana|rōmaji|romaji|pinyin|jyutping|'
    r'lenited|eclipsed|mutation|only used in|used in|see|used as|female equivalent|male equivalent|'
    r'supine|verbal noun|agent noun)\b.*\bof\b', re.I)

ARTICLES = ('a ', 'an ', 'the ', 'to ', 'some ', 'one who ', 'one that ', 'someone who ', 'something that ',
            'act of ', 'state of ', 'quality of ')


def clean_gloss_items(gloss: str, max_words: int = 4):
    """Split a Wiktionary English gloss into short candidate English keys."""
    g = gloss
    g = CLASSIFIER_RE.sub('', g)
    for _ in range(3):
        g = PAREN_RE.sub('', g)
        g = BRACKET_RE.sub('', g)
    g = g.replace('’', "'")
    items = []
    for part in re.split(r'[;,]|\bor\b', g):
        p = part.strip().strip('.').strip()
        if not p:
            continue
        pl = p.lower()
        changed = True
        while changed:
            changed = False
            for a in ARTICLES:
                if pl.startswith(a) and len(pl) > len(a):
                    pl = pl[len(a):]
                    changed = True
        pl = re.sub(r'\s+', ' ', pl).strip(' .,;:!?"\'')
        if not pl or len(pl) > 50:
            continue
        if len(pl.split()) > max_words:
            continue
        if any(ch.isdigit() for ch in pl):
            continue
        items.append(pl)
    return items


# canonical grammatical features (bit positions are assigned in Rust from this list order)
FEATURES = [
    'sg', 'pl', 'du',
    'nom', 'gen', 'dat', 'acc', 'ins', 'loc', 'abl', 'all', 'voc', 'poss', 'part', 'ess', 'ine', 'ela', 'ade', 'com', 'tra',
    'pres', 'past', 'fut', 'perf', 'impf', 'aor',
    'ind', 'subj', 'cond', 'imp', 'inf', 'ptcp', 'ger', 'opt',
    'act', 'pass',
    'p1', 'p2', 'p3',
    'masc', 'fem', 'neut',
    'cmp', 'sup',
    'neg', 'def', 'indef', 'dim', 'aug', 'formal', 'informal', 'formof', 'alt', 'archaic', 'refl', 'obj',
]
FEATURE_BIT = {f: i for i, f in enumerate(FEATURES)}

TAG_MAP = {
    'singular': 'sg', 'plural': 'pl', 'dual': 'du', 'singulative': 'sg', 'collective': 'pl',
    'nominative': 'nom', 'genitive': 'gen', 'dative': 'dat', 'accusative': 'acc', 'instrumental': 'ins',
    'locative': 'loc', 'ablative': 'abl', 'allative': 'all', 'illative': 'all', 'lative': 'all', 'vocative': 'voc',
    'possessive': 'poss', 'singular-possessive': 'poss', 'plural-possessive': 'poss', 'possessed-form': 'poss',
    'partitive': 'part', 'essive': 'ess', 'inessive': 'ine', 'elative': 'ela', 'adessive': 'ade', 'comitative': 'com',
    'translative': 'tra', 'prepositional': 'loc', 'oblique': 'acc', 'objective': 'acc', 'ergative': 'nom',
    'absolutive': 'nom', 'sublative': 'all', 'superessive': 'loc', 'delative': 'abl', 'terminative': 'all',
    'present': 'pres', 'non-past': 'pres', 'past': 'past', 'preterite': 'past', 'future': 'fut', 'future-i': 'fut',
    'future-ii': 'fut', 'perfect': 'perf', 'pluperfect': 'perf', 'imperfect': 'impf', 'aorist': 'aor',
    'perfective': 'perf', 'imperfective': 'impf', 'progressive': 'impf', 'continuative': 'impf', 'habitual': 'impf',
    'indicative': 'ind', 'subjunctive': 'subj', 'subjunctive-i': 'subj', 'subjunctive-ii': 'subj',
    'conditional': 'cond', 'conditional-ii': 'cond', 'imperative': 'imp', 'jussive': 'imp', 'infinitive': 'inf',
    'participle': 'ptcp', 'gerund': 'ger', 'supine': 'ptcp', 'optative': 'opt', 'potential': 'cond',
    'active': 'act', 'passive': 'pass', 'middle': 'pass',
    'first-person': 'p1', 'second-person': 'p2', 'third-person': 'p3',
    'masculine': 'masc', 'feminine': 'fem', 'neuter': 'neut', 'common-gender': 'masc',
    'comparative': 'cmp', 'superlative': 'sup',
    'negative': 'neg', 'connegative': 'neg', 'definite': 'def', 'indefinite': 'indef', 'diminutive': 'dim',
    'augmentative': 'aug', 'formal': 'formal', 'polite': 'formal', 'informal': 'informal', 'familiar': 'informal',
    'form-of': 'formof', 'alt-of': 'alt', 'alternative': 'alt', 'archaic': 'archaic', 'obsolete': 'archaic',
    'dated': 'archaic', 'historic': 'archaic', 'reflexive': 'refl', 'object-singular': 'obj', 'object-plural': 'obj',
    'object-third-person': 'obj', 'object-first-person': 'obj', 'object-second-person': 'obj',
}

# tags that make a form unusable as a lookup key
BAD_FORM_TAGS = {'romanization', 'inflection-template', 'table-tags', 'class', 'canonical', 'transliteration',
                 'Rōmaji', 'pinyin', 'error-unrecognized-form', 'multiword-construction', 'includes-article',
                 'combined-form', 'stem', 'misspelling', 'nonstandard', 'proscribed', 'rare', 'humorous',
                 'pronunciation-spelling', 'eye-dialect', 'Cyrillic', 'Latin', 'hangeul', 'katakana', 'hiragana'}

POS_MAP = {
    'noun': 'n', 'name': 'name', 'verb': 'vb', 'adj': 'adj', 'adv': 'adv', 'pron': 'pron', 'num': 'num',
    'det': 'det', 'article': 'det', 'prep': 'prep', 'postp': 'prep', 'conj': 'conj', 'intj': 'interj',
    'particle': 'part', 'phrase': 'phrase', 'prep_phrase': 'phrase', 'proverb': 'phrase', 'suffix': 'suf',
    'prefix': 'pref', 'character': 'char', 'symbol': 'sym', 'contraction': 'contr', 'classifier': 'cls',
    'counter': 'cls', 'adnominal': 'adj', 'root': 'root', 'affix': 'affix', 'infix': 'affix', 'interfix': 'affix',
    'circumfix': 'affix', 'combining_form': 'affix', 'preverb': 'pref', 'punct': 'punct', 'syllable': 'syl',
    'romanization': 'rom', 'soft-redirect': 'redir', 'circumpos': 'prep', 'stem': 'root',
}


def canon_tags(tags):
    out = set()
    for t in tags:
        c = TAG_MAP.get(t)
        if c:
            out.add(c)
    return out


def tags_to_bits(tags):
    bits = 0
    for t in tags:
        b = FEATURE_BIT.get(t)
        if b is not None:
            bits |= 1 << b
    return bits
