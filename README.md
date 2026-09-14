# tolkien — any language → Quenya, as a Unix pipe

`tolkien` reads text on **stdin** and writes a Quenya rendering on **stdout**.
It is a single statically linked Rust binary with every dictionary, grammar
table and language model compiled in: no network, no files, no runtime.

```
$ echo 'The stars shine over the sea.' | tolkien
I eleni silir or i ëar.

$ echo 'Gwiazdy świecą nad morzem.' | tolkien
Eleni silir or ëar.

$ echo '王は女王に剣を与えた。' | tolkien
aran antanë tárin macil.

$ echo 'Farewell, star-friend!' | tolkien --tengwar
                (Tengwar in the CSUR private-use block; needs a Tengwar font)
```

The input may be in (almost) any human language. The tool detects the
language, analyses the words with a multilingual dictionary derived from
Wiktionary, maps them to English lemmas, and renders the sentence with a
hand-written Quenya grammar engine over the Eldamo Neo-Quenya lexicon.
Everything works line by line, preserving line structure, punctuation,
Markdown markup, code spans, URLs and numbers, so it is safe on whole
documents (`tolkien < README.md`).

## Usage

```
tolkien [OPTIONS] [FILE ...]        (reads stdin when no FILE is given)

  --from LANG        Source language code (ISO 639-1/-3, e.g. en, pl, de, ja); default: auto-detect
  --to TARGET        quenya (default) or sindarin (experimental: Sindarin words on Quenya grammar)
  --tengwar          Write the output in Tengwar (Unicode CSUR private-use block U+E000–U+E07F)
  --numerals         Spell digits as Quenya numerals (3 → neldë)
  --adapt            Adapt untranslatable words/names to Quenya phonology
  --mark-unknown     Wrap untranslatable words in ⟦ ⟧
  --explain          Print per-word analysis (tag, English key, output) to stderr
  --detect           Only print the detected language of the input and exit
  --list-langs       List the source languages compiled into the binary
  -h, --help / -V, --version
```

Debugging aids: `--explain` shows how each word was tagged and translated;
`--senses --from LANG word…` dumps the dictionary readings and Quenya
candidates of words; `TOLKIEN_DEBUG=1` prints tagger emissions and the
phrase structure of each clause to stderr.

## What it does

1. **Tokenising** — words, numbers, punctuation, Markdown/HTML/URL/code
   spans (protected and copied through), CJK runs (segmented with a
   dynamic-programming word/particle segmenter for Chinese and Japanese,
   postposition splitting for Korean).
2. **Language detection** — trigram model (whatlang) combined with weighted
   dictionary coverage (function words — articles, pronouns, prepositions —
   count more than content words, so `Der Mann kam.` is German although
   every word is also an English dictionary entry) over the ~5 000 language
   codes present in the lexicon; documents are detected as a whole and
   single lines are re-detected when the document language explains them
   poorly; can be forced with `--from`.
3. **Lexical analysis** — each surface form is looked up in a compressed
   FST dictionary of *form → English lemma + inflection features* built
   from Wiktionary (all languages), with suffix-rule guessing for unknown
   inflected forms (Slavic/Romance/Germanic endings, Japanese conjugations,
   Korean endings, Turkish agglutination…). English lemmas without a Quenya
   word fall back to Wiktionary synonyms of the same part of speech
   (*expect* → *hope* → *mer-*), also for their inflected forms.
4. **Tagging and parsing** — a Viterbi part-of-speech tagger over the
   candidate readings, then a shallow parser into noun phrases, verb groups,
   prepositional phrases, coordination, questions, relative clauses (*i*),
   complement clauses (*sa*), German verb-final subordinate clauses.
5. **Quenya generation** — nouns are declined (10 cases, singular/plural/
   dual/partitive), verbs conjugated (aorist, present, past, perfect, future,
   with subject/object pronominal suffixes), adjectives agree in number,
   adverbs are derived from adjectives (*-vë*), possessives become suffixes
   (*parmanya*), articles, prepositions and word order follow Quenya usage
   (*ma* questions, fronted interrogatives, *ná* copula, *ëa* existentials,
   pronominal object suffixes); case-marked source languages keep their
   argument structure regardless of word order (*Книгу читает мальчик*).
6. **Untranslatable words** — proper names, acronyms (`IO`, `API`), single
   letters and words without a Quenya equivalent are kept verbatim
   (optionally adapted to Quenya phonology with `--adapt`); names in
   non-Latin scripts and protected tokens (code spans, URLs, identifiers)
   receive a preposition instead of a case ending (*mi 東京*, *mi `zstd.h`*)
   and stay at their place in the sentence
   (``Run `make` in the `src` directory`` → ``Á norë `make` i `src` nyardassë``).

The Quenya vocabulary is Neo-Quenya as curated by Eldamo (attested Quenya
plus the standard neologisms), with Sindarin as an experimental alternative
vocabulary. Grammar follows Thorsten Renk's *Quetin i lambë eldaiva* and
Helge Fauskanger's Quenya course conventions.

## Building

Prerequisites: Rust ≥ 1.75 and the generated data blobs in `data/`
(see *Regenerating the data*; they are not part of the git repository).

```
cargo build --release --target x86_64-unknown-freebsd -p tolkien   # FreeBSD
cargo build --release -p tolkien                                    # Linux/macOS
```

The binary is `target/<triple>/release/tolkien` (~100 MB, most of it the
compressed dictionaries). A prebuilt, stripped FreeBSD/amd64 binary is
committed as `tolkien-freebsd-amd64` (Git LFS).

Tests: `cargo test --release --target <triple> -p tolkien` (unit tests) and
`tests/regress/run.sh` (golden translations for en/de/es/fr/pl/ru/zh/ja/ko).

## Regenerating the data

The `data/` directory is produced from two public sources:

* **Eldamo** (`eldamo-data.xml`, https://eldamo.org, CC BY 4.0) →
  `tools/eldamo_extract.py` → `research/build/lexicon_q.json`,
  `lexicon_s.json`.
* **Wiktionary** via the `wiktextract` dump of kaikki.org
  (`raw-wiktextract-data.jsonl.gz`, ~2.9 GB compressed, CC BY-SA 3.0) →
  `tools/kaikki_reduce.py` → `kaikki_passA.py` (English records, translation
  pairs, foreign records) → `kaikki_passB.py` (English link graph:
  inflected forms and POS-tagged synonym links; its optional fourth argument
  lists the key files whose lemmas need no synonym links, normally the
  Quenya lexicon and the function words) → `kaikki_passC.py` (filter and
  shard by language) →
  `kaikki_cjk_chars.py` (CJK single-character entries) → `kaikki_passD.py`
  (join, rank, cap per language) → `research/build/lex.tsv`.
* `cargo run --release -p datagen -- research/build tools data` compiles
  both into the FST/blob files that `crates/tolkien/src/data.rs` embeds with
  `include_bytes!`.

The whole pipeline needs ~40 GB of scratch space and about an hour on a
16-core machine; each tool's docstring documents its arguments.

## Layout

```
crates/tolkien/   the translator (tokenize, detect, analyze, morph, cjk, translate, quenya, tengwar)
crates/datagen/   builds the embedded data files from research/build
crates/tkdata/    binary formats shared by datagen and tolkien
tools/            Python data pipeline (Eldamo + Wiktionary)
tests/regress/    golden-output regression suite
```

## Stress test

The final binary was run over every text-like file on the development
machine (FreeBSD 15, 16 cores), one process per file, with a 300 s timeout:

| corpus | files | translated | empty input | errors / timeouts | lines |
|---|---:|---:|---:|---:|---:|
| `*.md` (all of `/`) | 9 358 | 9 310 | 48 | 0 | 1.25 M |
| PDF (`pdftotext`) | 4 398 | 4 335 | 63 | 0 | 3.43 M |
| other text files | 1 000 664 | 981 493 | 19 171 | 0 | 245.8 M |

Every run produced exactly one output line per input line, nothing on
stderr and no crash. The slowest file (a 523 KB single-line Go byte table)
takes 0.15 s; a 20 000-line Polish novel about 6 s.

## Limitations

* Quenya is a reconstructed language with a few thousand attested words;
  technical vocabulary is often missing and is then left in the source
  language (mark it with `--mark-unknown`).
* Word-sense disambiguation is heuristic; idioms, humour and poetry will
  be rendered literally.
* The Sindarin target substitutes Sindarin vocabulary but still applies
  Quenya morphology — it is a preview, not Sindarin grammar.
* Detection of very short inputs in closely related languages (e.g.
  Czech/Slovak, Spanish/Portuguese) can be wrong; use `--from`.
* The embedded dictionary caps the number of inflected forms per language
  (see `CAP_MAX` in `tools/kaikki_passD.py`); for languages with very large
  paradigms (Arabic, Finnish, Hungarian) rare person/number forms may be
  missing and are then left untranslated (*رأوا*). Unvocalised Arabic and
  Hebrew forms are ambiguous between readings; the singular one is preferred.

## Licenses

The code is licensed under the Apache License 2.0 (see `LICENSE`).
The embedded data is derived from Eldamo (© Paul Strack, CC BY 4.0) and
from Wiktionary (CC BY-SA 3.0); see `NOTICE` for the attribution and the
licenses of the Rust dependencies.
