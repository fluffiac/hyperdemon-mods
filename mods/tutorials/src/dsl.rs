//! The `.training` format: one training per file, one directive per line.
//! Reference: `docs/TRAINING_FORMAT.md`.
//!
//! ```text
//! training 0xDA100FD6 "DASH+SLIDE"
//! builtin
//! step "-Tap {jump} while in the air to do a short DASH." until dash >= 3
//! keep 1 gem at 0 10 -20 every 1
//! ```

use std::path::{Path, PathBuf};

use hd_sdk::game::Bind;
use hd_sdk::training;
use hd_sdk::world::Spawnable;

/// The game shows at most five hint lines under the title.
pub const MAX_STEPS: usize = 5;

#[derive(Debug)]
pub struct Training {
    pub id: u32,
    /// HUD title (may be empty, like the built-in score training).
    pub title: String,
    /// The game's own training: the server lists it and serves its clip.
    pub builtin: bool,
    pub menu: Menu,
    /// Custom only. Without one (or while the file is missing) the training
    /// plays on a built-in clip (`FALLBACK_CLIP` in `lib.rs`).
    pub clip: Option<PathBuf>,
    pub thumbnail: Option<PathBuf>,
    pub steps: Vec<Step>,
    pub spawns: Vec<Spawn>,
    pub show_score: bool,
    pub drain_score: bool,
}

/// Row in the TRAINING menu (custom trainings only).
#[derive(Debug, PartialEq)]
pub struct Menu {
    pub title: String,
    pub subtitle: String,
    /// Shown as the clip length, in seconds.
    pub length: u32,
}

#[derive(Debug)]
pub struct Step {
    pub text: Text,
    pub goal: Option<Goal>,
    /// Only shown (and only counted) once the player took over.
    pub playing_only: bool,
}

#[derive(Debug)]
pub struct Goal {
    pub expr: Expr,
    pub target: i32,
}

#[derive(Debug, PartialEq)]
pub enum Expr {
    Counter { name: String, offset: usize },
    Score,
    Int(i32),
    Sum(Vec<Expr>),
    /// `a > n`: 1 if true, else 0.
    Above(Box<Expr>, i32),
}

#[derive(Debug, PartialEq)]
pub struct Spawn {
    pub kind: Spawnable,
    pub count: i32,
    pub at: Option<[f32; 3]>,
    /// Seconds to wait once the count drops below `count`, and between spawns.
    pub every: f32,
    /// Only once steps `1..=after` are done (0: always).
    pub after: usize,
}

/// Hint text with `{bind}` placeholders for key names.
#[derive(Debug, PartialEq)]
pub struct Text(Vec<Piece>);

#[derive(Debug, PartialEq)]
enum Piece {
    Lit(String),
    Key(Bind),
}

impl Text {
    /// The text with placeholders replaced by `key(bind)`.
    pub fn render(&self, key: impl Fn(Bind) -> String) -> String {
        self.0
            .iter()
            .map(|p| match p {
                Piece::Lit(s) => s.clone(),
                Piece::Key(b) => key(*b),
            })
            .collect()
    }
}

impl Expr {
    pub fn eval(&self, counter: &impl Fn(usize) -> i32, score: &impl Fn() -> f32) -> i32 {
        match self {
            Expr::Counter { offset, .. } => counter(*offset),
            Expr::Score => score() as i32,
            Expr::Int(n) => *n,
            Expr::Sum(terms) => terms.iter().map(|t| t.eval(counter, score)).sum(),
            Expr::Above(e, n) => i32::from(e.eval(counter, score) > *n),
        }
    }

    /// World offsets of every counter the expression reads.
    pub fn counters(&self, out: &mut Vec<usize>) {
        match self {
            Expr::Counter { offset, .. } => out.push(*offset),
            Expr::Sum(terms) => terms.iter().for_each(|t| t.counters(out)),
            Expr::Above(e, _) => e.counters(out),
            Expr::Score | Expr::Int(_) => {}
        }
    }
}

impl Training {
    pub fn has_objective(&self) -> bool {
        self.steps.iter().any(|s| s.goal.is_some())
    }
}

pub fn parse_file(path: &Path) -> Result<Training, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse(&text, path.parent().unwrap_or(Path::new(".")))
}

/// Parses one training. Relative paths are resolved against `dir`.
pub fn parse(src: &str, dir: &Path) -> Result<Training, String> {
    let mut t: Option<Training> = None;
    let mut menu_seen = false;
    for (n, raw) in src.lines().enumerate() {
        let line = n + 1;
        let err = |m: String| format!("line {line}: {m}");
        let toks = tokenize(raw).map_err(err)?;
        if toks.is_empty() {
            continue;
        }
        let mut c = Cursor { toks: &toks, pos: 0 };
        let head = c.word().map_err(err)?;
        if head != "training" && t.is_none() {
            return Err(err(format!("expected `training <id> \"<title>\"` first, found `{head}`")));
        }
        match head.as_str() {
            "training" => {
                if t.is_some() {
                    return Err(err("only one `training` per file".into()));
                }
                let id = c.id().map_err(err)?;
                let title = c.string().map_err(err)?;
                t = Some(Training {
                    id,
                    menu: Menu { title: title.clone(), subtitle: "custom".into(), length: 0 },
                    title,
                    builtin: false,
                    clip: None,
                    thumbnail: None,
                    steps: Vec::new(),
                    spawns: Vec::new(),
                    show_score: false,
                    drain_score: false,
                });
            }
            "builtin" => t.as_mut().unwrap().builtin = true,
            "menu" => {
                let tr = t.as_mut().unwrap();
                tr.menu.title = c.string().map_err(err)?;
                if c.peek().is_some() {
                    tr.menu.subtitle = c.string().map_err(err)?;
                }
                if c.peek().is_some() {
                    tr.menu.length = c.number().map_err(err)? as u32;
                }
                menu_seen = true;
            }
            "clip" => t.as_mut().unwrap().clip = Some(dir.join(c.path().map_err(err)?)),
            "thumbnail" => t.as_mut().unwrap().thumbnail = Some(dir.join(c.path().map_err(err)?)),
            "step" => {
                let text = parse_text(&c.string().map_err(err)?).map_err(err)?;
                let mut step = Step { text, goal: None, playing_only: false };
                while let Some(kw) = c.peek_word() {
                    c.pos += 1;
                    match kw {
                        "until" => {
                            let expr = c.expr().map_err(err)?;
                            c.symbol(">=").map_err(err)?;
                            let target = c.number().map_err(err)? as i32;
                            step.goal = Some(Goal { expr, target });
                        }
                        "when" => {
                            c.keyword("playing").map_err(err)?;
                            step.playing_only = true;
                        }
                        other => return Err(err(format!("unexpected `{other}` (expected `until` or `when playing`)"))),
                    }
                }
                t.as_mut().unwrap().steps.push(step);
            }
            "keep" => {
                let count = c.number().map_err(err)?;
                let kind_name = c.word().map_err(err)?;
                let kind = Spawnable::from_name(&kind_name).ok_or_else(|| {
                    err(format!(
                        "unknown kind `{kind_name}` (one of: {})",
                        Spawnable::ALL.map(|k| k.name()).join(", ")
                    ))
                })?;
                let mut sp = Spawn { kind, count: count as i32, at: None, every: 1.0, after: 0 };
                while let Some(kw) = c.peek_word() {
                    c.pos += 1;
                    match kw {
                        "at" => {
                            if kind.default_position().is_none() {
                                return Err(err(format!("`{kind_name}` picks its own position; `at` is not allowed")));
                            }
                            sp.at = Some([c.number().map_err(err)?, c.number().map_err(err)?, c.number().map_err(err)?]);
                        }
                        "every" => sp.every = c.number().map_err(err)?,
                        "after" => {
                            c.keyword("step").map_err(err)?;
                            sp.after = c.number().map_err(err)? as usize;
                        }
                        other => return Err(err(format!("unexpected `{other}` (expected `at`, `every` or `after step`)"))),
                    }
                }
                if sp.count < 1 || sp.every < 0.0 {
                    return Err(err("count must be at least 1 and `every` not negative".into()));
                }
                if kind == Spawnable::BluePowerup && sp.count != 1 {
                    return Err(err("there is only one bluepowerup; use `keep 1 bluepowerup`".into()));
                }
                t.as_mut().unwrap().spawns.push(sp);
            }
            "show" => {
                c.keyword("score").map_err(err)?;
                t.as_mut().unwrap().show_score = true;
            }
            "drain" => {
                c.keyword("score").map_err(err)?;
                t.as_mut().unwrap().drain_score = true;
            }
            other => return Err(err(format!("unknown directive `{other}`"))),
        }
        if let Some(extra) = c.peek() {
            return Err(err(format!("unexpected {extra} at end of line")));
        }
    }
    let t = t.ok_or("empty file: expected `training <id> \"<title>\"`")?;
    if t.steps.len() > MAX_STEPS {
        return Err(format!("{} steps; the game shows at most {MAX_STEPS}", t.steps.len()));
    }
    if let Some(sp) = t.spawns.iter().find(|s| s.after > t.steps.len()) {
        return Err(format!("`keep … after step {}` but there are only {} steps", sp.after, t.steps.len()));
    }
    if t.builtin && (t.clip.is_some() || menu_seen) {
        return Err("`builtin` trainings come from the game's server; drop `clip`/`menu`".into());
    }
    Ok(t)
}

/// Splits `{jump}`-style placeholders out of hint text (`{{` / `}}` escape).
fn parse_text(s: &str) -> Result<Text, String> {
    let mut pieces = Vec::new();
    let mut lit = String::new();
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                lit.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                lit.push('}');
            }
            '{' => {
                let name: String = chars.by_ref().take_while(|&c| c != '}').collect();
                let bind = Bind::from_name(&name).ok_or_else(|| {
                    format!("unknown key `{{{name}}}` (one of: {})", Bind::ALL.map(|b| b.name()).join(", "))
                })?;
                if !lit.is_empty() {
                    pieces.push(Piece::Lit(std::mem::take(&mut lit)));
                }
                pieces.push(Piece::Key(bind));
            }
            '}' => return Err("unmatched `}` in text (write `}}`)".into()),
            c => lit.push(c),
        }
    }
    if !lit.is_empty() {
        pieces.push(Piece::Lit(lit));
    }
    Ok(Text(pieces))
}

#[derive(Debug, PartialEq)]
enum Tok {
    Bare(String),
    Str(String),
    Sym(&'static str),
}

impl std::fmt::Display for Tok {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Tok::Bare(s) => write!(f, "`{s}`"),
            Tok::Str(s) => write!(f, "\"{s}\""),
            Tok::Sym(s) => write!(f, "`{s}`"),
        }
    }
}

fn tokenize(line: &str) -> Result<Vec<Tok>, String> {
    let mut out = Vec::new();
    let mut it = line.char_indices().peekable();
    while let Some(&(i, ch)) = it.peek() {
        match ch {
            '#' => break,
            c if c.is_whitespace() => {
                it.next();
            }
            '"' => {
                it.next();
                let mut s = String::new();
                loop {
                    match it.next() {
                        Some((_, '"')) => break,
                        Some((_, '\\')) => match it.next() {
                            Some((_, c @ ('"' | '\\'))) => s.push(c),
                            Some((_, c)) => return Err(format!("unknown escape `\\{c}`")),
                            None => return Err("unterminated string".into()),
                        },
                        Some((_, c)) => s.push(c),
                        None => return Err("unterminated string".into()),
                    }
                }
                out.push(Tok::Str(s));
            }
            '(' | ')' | '+' => {
                it.next();
                out.push(Tok::Sym(match ch {
                    '(' => "(",
                    ')' => ")",
                    _ => "+",
                }));
            }
            '>' => {
                it.next();
                if it.peek().map(|&(_, c)| c) == Some('=') {
                    it.next();
                    out.push(Tok::Sym(">="));
                } else {
                    out.push(Tok::Sym(">"));
                }
            }
            _ => {
                let start = i;
                let mut end = line.len();
                while let Some(&(j, c)) = it.peek() {
                    if c.is_whitespace() || "\"#()+>".contains(c) {
                        end = j;
                        break;
                    }
                    it.next();
                }
                out.push(Tok::Bare(line[start..end].to_string()));
            }
        }
    }
    Ok(out)
}

struct Cursor<'a> {
    toks: &'a [Tok],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<&'a Tok> {
        self.toks.get(self.pos)
    }

    fn peek_word(&self) -> Option<&'a str> {
        match self.peek() {
            Some(Tok::Bare(s)) => Some(s),
            _ => None,
        }
    }

    fn next(&mut self, what: &str) -> Result<&Tok, String> {
        let t = self.toks.get(self.pos).ok_or_else(|| format!("expected {what} at end of line"))?;
        self.pos += 1;
        Ok(t)
    }

    fn word(&mut self) -> Result<String, String> {
        match self.next("a word")? {
            Tok::Bare(s) => Ok(s.clone()),
            t => Err(format!("expected a word, found {t}")),
        }
    }

    fn keyword(&mut self, kw: &str) -> Result<(), String> {
        match self.next(&format!("`{kw}`"))? {
            Tok::Bare(s) if s == kw => Ok(()),
            t => Err(format!("expected `{kw}`, found {t}")),
        }
    }

    fn symbol(&mut self, sym: &str) -> Result<(), String> {
        match self.next(&format!("`{sym}`"))? {
            Tok::Sym(s) if *s == sym => Ok(()),
            t => Err(format!("expected `{sym}`, found {t}")),
        }
    }

    fn string(&mut self) -> Result<String, String> {
        match self.next("a \"string\"")? {
            Tok::Str(s) => Ok(s.clone()),
            t => Err(format!("expected a \"string\", found {t}")),
        }
    }

    /// A quoted or bare path.
    fn path(&mut self) -> Result<String, String> {
        match self.next("a path")? {
            Tok::Str(s) | Tok::Bare(s) => Ok(s.clone()),
            t => Err(format!("expected a path, found {t}")),
        }
    }

    fn number(&mut self) -> Result<f32, String> {
        match self.next("a number")? {
            Tok::Bare(s) => s.trim_end_matches('s').parse().map_err(|_| format!("expected a number, found `{s}`")),
            t => Err(format!("expected a number, found {t}")),
        }
    }

    /// Clip id: decimal or `0x` hex u32.
    fn id(&mut self) -> Result<u32, String> {
        let s = self.word()?;
        let v = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            Some(h) => u32::from_str_radix(&h.replace('_', ""), 16),
            None => s.replace('_', "").parse(),
        };
        v.map_err(|_| format!("`{s}` is not a clip id (decimal or 0x hex, 32 bit)"))
    }

    /// expr := term ('+' term)* ; term := atom ('>' INT)? ;
    /// atom := COUNTER | 'score' | INT | '(' expr ')'
    fn expr(&mut self) -> Result<Expr, String> {
        let mut terms = vec![self.term()?];
        while self.peek() == Some(&Tok::Sym("+")) {
            self.pos += 1;
            terms.push(self.term()?);
        }
        Ok(if terms.len() == 1 { terms.pop().unwrap() } else { Expr::Sum(terms) })
    }

    fn term(&mut self) -> Result<Expr, String> {
        let atom = self.atom()?;
        if self.peek() == Some(&Tok::Sym(">")) {
            self.pos += 1;
            let n = self.number()? as i32;
            return Ok(Expr::Above(Box::new(atom), n));
        }
        Ok(atom)
    }

    fn atom(&mut self) -> Result<Expr, String> {
        match self.next("a counter")? {
            Tok::Sym("(") => {
                let e = self.expr()?;
                self.symbol(")")?;
                Ok(e)
            }
            Tok::Bare(s) if s == "score" => Ok(Expr::Score),
            Tok::Bare(s) if s.parse::<i32>().is_ok() => Ok(Expr::Int(s.parse().unwrap())),
            Tok::Bare(s) => match training::counter_offset(s) {
                Some(offset) => Ok(Expr::Counter { name: s.clone(), offset }),
                None => Err(format!("unknown counter `{s}` (see docs/TRAINING_FORMAT.md)")),
            },
            t => Err(format!("expected a counter, found {t}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(src: &str) -> Result<Training, String> {
        parse(src, Path::new("dir"))
    }

    #[test]
    fn parses_a_custom_training() {
        let t = p(r#"
            # comment
            training 0xF0000001 "TRIPLE STUN"
            menu "TRIPLE STUN" "custom" 12
            clip triple_stun.hdclip
            step "-Three ENEMY 1s at once."
            step "-Finish them with a {jump}." until scuttles_boost >= 6 when playing
            keep 3 spawner every 0.5s after step 1
            keep 1 gem at 10 10 -20
            show score
        "#)
        .unwrap();
        assert_eq!(t.id, 0xF000_0001);
        assert_eq!(t.menu, Menu { title: "TRIPLE STUN".into(), subtitle: "custom".into(), length: 12 });
        assert_eq!(t.clip.as_deref(), Some(Path::new("dir/triple_stun.hdclip")));
        assert_eq!(t.steps.len(), 2);
        assert!(t.steps[0].goal.is_none());
        let g = t.steps[1].goal.as_ref().unwrap();
        assert_eq!((&g.expr, g.target), (&Expr::Counter { name: "scuttles_boost".into(), offset: 0x104 }, 6));
        assert!(t.steps[1].playing_only);
        assert_eq!(t.steps[1].text.render(|b| b.name().to_uppercase()), "-Finish them with a JUMP.");
        assert_eq!(t.spawns[0], Spawn { kind: Spawnable::Spawner, count: 3, at: None, every: 0.5, after: 1 });
        assert_eq!(t.spawns[1].at, Some([10.0, 10.0, -20.0]));
        assert!(t.show_score && !t.drain_score && !t.builtin);
    }

    #[test]
    fn clip_is_optional() {
        let t = p("training 0xF0000001 \"BLANK\"\nstep \"x\"").unwrap();
        assert!(t.clip.is_none() && !t.builtin);
    }

    #[test]
    fn parses_expressions() {
        let t = p(r#"training 1 ""
            builtin
            step "-x" until (bomb1_lasered > 0) + (bomb2_lasered > 0) >= 2
            step "-y" until score >= 25"#)
        .unwrap();
        let e = &t.steps[0].goal.as_ref().unwrap().expr;
        let counter = |o: usize| if o == 0xd8 { 5 } else { 0 };
        assert_eq!(e.eval(&counter, &|| 0.0), 1);
        let mut offs = Vec::new();
        e.counters(&mut offs);
        assert_eq!(offs, [0xd8, 0xdc]);
        assert_eq!(t.steps[1].goal.as_ref().unwrap().expr.eval(&counter, &|| 25.9), 25);
    }

    #[test]
    fn reports_errors_with_line_numbers() {
        let cases = [
            ("step \"x\"", "line 1: expected `training"),
            ("training 1 \"a\"\nclip a\nstep \"{jmup}\"", "line 3: unknown key `{jmup}`"),
            ("training 1 \"a\"\nclip a\nstep \"x\" until dahs >= 3", "line 3: unknown counter `dahs`"),
            ("training 1 \"a\"\nclip a\nkeep 1 spawner at 0 0 0", "line 3: `spawner` picks its own position"),
            ("training 1 \"a\"\nclip a\nkeep 2 bluepowerup", "line 3: there is only one bluepowerup"),
            ("training 1 \"a\"\nclip a\nshow scroe", "line 3: expected `score`"),
            ("training 1 \"a\"\nclip a\nstep \"x\" until dash > 3", "line 3: expected `>=`"),
            ("training 1 \"a\"\nbuiltin\nclip a", "`builtin` trainings come from the game's server"),
            ("training 1 \"a\"\nclip a\nkeep 1 spawner after step 1", "only 0 steps"),
            ("training 1 \"unterminated", "line 1: unterminated string"),
        ];
        for (src, want) in cases {
            let got = p(src).unwrap_err();
            assert!(got.contains(want), "{src:?}: got {got:?}, want {want:?}");
        }
    }

    #[test]
    fn escapes_braces() {
        assert_eq!(parse_text("{{a}} {fire}").unwrap().render(|_| "LMB".into()), "{a} LMB");
    }

    /// Every bundled training parses.
    #[test]
    fn bundled_trainings_parse() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("game").join("trainings");
        let parse_dir = |dir: &Path| -> Vec<Training> {
            std::fs::read_dir(dir)
                .unwrap()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "training"))
                .map(|p| parse_file(&p).unwrap_or_else(|err| panic!("{}: {err}", p.display())))
                .collect()
        };
        let builtin = parse_dir(&dir.join("builtin"));
        let custom = parse_dir(&dir);
        assert_eq!(builtin.len(), 13);
        assert!(builtin.iter().all(|t| t.builtin), "trainings/builtin/ holds only built-in trainings");
        assert_eq!(custom.len(), 3);
        assert!(custom.iter().all(|t| !t.builtin), "built-in trainings belong in trainings/builtin/");
    }
}
