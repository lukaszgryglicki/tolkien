//! tolkien — translate any-language text from stdin into Quenya (or Sindarin), offline.

mod analyze;
mod cjk;
mod data;
mod detect;
mod english;
mod morph;
mod quenya;
mod tengwar;
mod tokenize;
mod translate;

use std::io::{self, BufWriter, Read, Write};

use data::Target;
use translate::{Options, Translator};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
tolkien — translate text in any language into Tolkien's Elvish (Quenya), fully offline

USAGE:
    tolkien [OPTIONS] [FILE ...]        (reads stdin when no FILE is given)

OPTIONS:
    --from LANG        Source language code (ISO 639-1/-3, e.g. en, pl, de, ja); default: auto-detect
    --to TARGET        quenya (default) or sindarin (experimental: Sindarin words on Quenya grammar)
    --tengwar          Write the output in Tengwar (Unicode CSUR private-use block, needs a Tengwar font)
    --numerals         Spell digits as Quenya numerals (e.g. 3 → neldë)
    --adapt            Adapt untranslatable words/names to Quenya phonology
    --mark-unknown     Wrap untranslatable words in ⟦ ⟧
    --explain          Print per-word analysis (tag, English key, output) to stderr
    --detect           Only print the detected language of the input and exit
    --list-langs       List the source languages compiled into the binary
    -h, --help         Show this help
    -V, --version      Show version

EXAMPLES:
    echo 'The stars shine over the sea.' | tolkien
    echo 'Gwiazdy świecą nad morzem.' | tolkien --from pl
    tolkien --tengwar < poem.txt

Vocabulary: Eldamo (Paul Strack, CC BY 4.0) · Wiktionary (CC BY-SA 3.0) · detection: whatlang.
";

struct Cli {
    from: Option<String>,
    target: Target,
    tengwar: bool,
    numerals: bool,
    adapt: bool,
    mark_unknown: bool,
    explain: bool,
    detect_only: bool,
    list_langs: bool,
    senses: bool,
    files: Vec<String>,
}

fn parse_args() -> Result<Cli, String> {
    let mut cli = Cli { from: None, target: Target::Quenya, tengwar: false, numerals: false, adapt: false, mark_unknown: false, explain: false, detect_only: false, list_langs: false, senses: false, files: Vec::new() };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{}", HELP);
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("tolkien {}", VERSION);
                std::process::exit(0);
            }
            "--from" | "-f" => cli.from = Some(args.next().ok_or("--from needs a language code")?.to_lowercase()),
            "--to" | "-t" => {
                let t = args.next().ok_or("--to needs quenya|sindarin")?;
                cli.target = match t.to_lowercase().as_str() {
                    "q" | "quenya" => Target::Quenya,
                    "s" | "sindarin" => Target::Sindarin,
                    _ => return Err(format!("unknown target language '{}'", t)),
                };
            }
            "--tengwar" => cli.tengwar = true,
            "--numerals" => cli.numerals = true,
            "--adapt" => cli.adapt = true,
            "--mark-unknown" => cli.mark_unknown = true,
            "--explain" => cli.explain = true,
            "--detect" => cli.detect_only = true,
            "--list-langs" => cli.list_langs = true,
            "--senses" => cli.senses = true,
            "-" => cli.files.push("-".to_string()),
            _ if a.starts_with("--from=") => cli.from = Some(a["--from=".len()..].to_lowercase()),
            _ if a.starts_with("--to=") => {
                cli.target = match a["--to=".len()..].to_lowercase().as_str() {
                    "q" | "quenya" => Target::Quenya,
                    "s" | "sindarin" => Target::Sindarin,
                    other => return Err(format!("unknown target language '{}'", other)),
                }
            }
            _ if a.starts_with('-') && a.len() > 1 => return Err(format!("unknown option '{}'", a)),
            _ => cli.files.push(a),
        }
    }
    Ok(cli)
}

fn read_input(files: &[String]) -> io::Result<String> {
    let mut buf = Vec::new();
    if files.is_empty() {
        io::stdin().lock().read_to_end(&mut buf)?;
    } else {
        for f in files {
            if f == "-" {
                io::stdin().lock().read_to_end(&mut buf)?;
            } else {
                let mut fh = std::fs::File::open(f)?;
                fh.read_to_end(&mut buf)?;
            }
        }
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Split text into lines keeping the original line terminators.
fn lines_with_endings(text: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        match rest.find('\n') {
            Some(p) => {
                let (line, tail) = rest.split_at(p);
                let (line, end) = if line.ends_with('\r') { (&line[..line.len() - 1], "\r\n") } else { (line, "\n") };
                out.push((line, end));
                rest = &tail[1..];
            }
            None => {
                out.push((rest, ""));
                rest = "";
            }
        }
    }
    out
}

fn main() {
    let cli = match parse_args() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("tolkien: {}\n\n{}", e, HELP);
            std::process::exit(2);
        }
    };
    let data = data::Data::load();
    if cli.list_langs {
        let mut langs = data.langs();
        langs.sort();
        for (code, n) in langs {
            println!("{}\t{}", code, n);
        }
        return;
    }
    if cli.senses {
        // debugging aid: dump the analysis of each argument word
        let lang = cli.from.clone().unwrap_or_else(|| "en".to_string());
        let mut an = analyze::Analyzer::new(&data, &lang, cli.target);
        for wd in &cli.files {
            let a = an.analyze(wd);
            println!("== {} [{}] role={:?} key={} guessed={}", wd, lang, a.role, a.role_key, a.guessed);
            for (r, k, rel) in &a.roles {
                println!("  role {:?} key={} rel={:.2}", r, k, rel);
            }
            for s in &a.senses {
                let ts: Vec<String> = s.tagsets.iter().map(|t| tkdata::features::describe(*t)).collect();
                println!("  sense {:<20} w={:<3} pos={:<2} tags={}", s.key, s.weight, s.pos, ts.join(" | "));
            }
            for c in a.cands.iter().take(12) {
                println!("  cand {:<20} base={:<6.1} cat={:?} pos={} gloss={}", c.entry.form, c.base, c.cat, c.entry.pos, c.entry.gloss.chars().take(50).collect::<String>());
            }
        }
        return;
    }
    let text = match read_input(&cli.files) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("tolkien: cannot read input: {}", e);
            std::process::exit(1);
        }
    };
    let lines = lines_with_endings(&text);

    // document-level language detection on the first chunk
    let mut sample = String::new();
    for (l, _) in lines.iter().take(400) {
        if sample.len() > 8192 {
            break;
        }
        sample.push_str(l);
        sample.push('\n');
    }
    let doc = detect::detect(&data, &sample, cli.from.as_deref());
    if cli.detect_only {
        println!("{}\t{:.2}\t{}", doc.lang, doc.hit_rate, doc.words);
        return;
    }
    let forced = cli.from.is_some();
    let doc_lang = if forced { cli.from.clone().unwrap() } else { doc.lang.clone() };

    let an = analyze::Analyzer::new(&data, &doc_lang, cli.target);
    let opts = Options { mark_unknown: cli.mark_unknown, adapt: cli.adapt, numerals: cli.numerals, explain: cli.explain };
    let mut tr = Translator::new(an, opts);

    let stdout = io::stdout();
    let mut out = BufWriter::with_capacity(1 << 16, stdout.lock());
    let stderr = io::stderr();
    let mut err = stderr.lock();
    let mut cur_lang = doc_lang.clone();
    for (line, end) in &lines {
        // per-line re-detection for multilingual documents
        if !forced && !tr.in_fence() && !translate::looks_like_code(line) {
            let words = line.split_whitespace().count();
            if words >= 3 {
                // only look for another language when the document language explains the line
                // poorly and the unexplained words look like words (not identifiers or numbers)
                let (doc_rate, unknown) = line_hit_rate(&data, &doc_lang, line);
                let want = if doc_rate < 0.8 && unknown >= 2 {
                    let d = detect::detect_ex(&data, line, Some(&doc_lang), true);
                    if d.lang != doc_lang && d.hit_rate >= 0.7 && d.hit_rate > doc_rate + 0.25 { d.lang.clone() } else { doc_lang.clone() }
                } else {
                    doc_lang.clone()
                };
                if want != cur_lang {
                    cur_lang = want;
                    tr.set_lang(&cur_lang);
                }
            }
        }
        let translated = tr.translate_line(line);
        let translated = if cli.tengwar { tengwar::transcribe(&translated) } else { translated };
        if out.write_all(translated.as_bytes()).and_then(|_| out.write_all(end.as_bytes())).is_err() {
            return; // closed pipe
        }
        if cli.explain {
            for l in &tr.explain_lines {
                let _ = writeln!(err, "{}", l);
            }
        }
    }
    let _ = out.flush();
}

/// Share of the line's words the lexicon of `lang` knows, and the number of unknown words
/// that look like natural-language words (letters only, not CamelCase).
fn line_hit_rate(data: &data::Data, lang: &str, line: &str) -> (f32, usize) {
    let mut n = 0usize;
    let mut hit = 0usize;
    let mut unknown = 0usize;
    for t in tokenize::tokenize(line) {
        if t.kind == tokenize::Kind::Word {
            n += 1;
            if detect::known(data, lang, t.text) {
                hit += 1;
            } else {
                let wordlike = t.text.chars().count() >= 2
                    && t.text.chars().all(|c| c.is_alphabetic())
                    && !t.text.chars().skip(1).any(|c| c.is_uppercase());
                if wordlike {
                    unknown += 1;
                }
            }
        }
    }
    if n == 0 {
        (1.0, 0)
    } else {
        (hit as f32 / n as f32, unknown)
    }
}
