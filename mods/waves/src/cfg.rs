//! `waves.cfg`: one directive per line, `#` starts a comment.
//!
//! ```text
//! arena radius 40
//! arena shrink off
//! enemy cap 12
//! wave every 30
//! powerup mercury every 20 after 10
//! powerup sulfur off
//! powerup mercury lasts 15
//! wave owl@0,-20
//! wave trilo snake@15,15
//! wave boss:3@0,30,0
//! ```
//!
//! Anything not mentioned keeps the game's own behaviour. The first `wave`
//! line replaces the game's whole spawn table.

/// A table entry type, as the game's sequencer (`FUN_1400de900`) reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kind(pub i32);

/// Owl variant 1: the boss of the game's last wave.
pub const BOSS: Kind = Kind(13);

/// Names for the types the sequencer spawns. Variant 1 of the other
/// managers is named `*_boss`; the game's own table never uses them, and
/// that they are bosses is a guess (see the docs).
pub const KINDS: &[(&str, i32)] = &[
    ("owl", 11),
    ("boss", 13),
    ("trilo", 12),
    ("trilo_boss", 14),
    ("snake", 10),
    ("snake_boss", 15),
    ("spider", 3),
    ("spider_boss", 8),
    ("centipede", 2),
    ("centipede_boss", 5),
    ("centipede_2", 9),
    ("skull", SKULL_TYPE),
    ("skull2", SKULL_TYPE + 2),
    ("skull3", SKULL_TYPE + 3),
];

/// Skulls are not in the game's table. The mod writes them as types the
/// sequencer skips (100 + `World::SpawnNearPlayer` kind) and spawns them
/// itself once the rest of their wave has spawned.
pub const SKULL_TYPE: i32 = 100;

/// Names the `.training` files use for the same enemies.
/// `owl_boss` is the boss's old name.
const ALIASES: &[(&str, &str)] =
    &[("spawner", "owl"), ("scuttles", "trilo"), ("snakes", "snake"), ("spiders", "spider"), ("owl_boss", "boss")];

impl Kind {
    pub fn from_name(s: &str) -> Option<Kind> {
        let s = ALIASES.iter().find(|(a, _)| a.eq_ignore_ascii_case(s)).map_or(s, |(_, n)| n);
        if let Some(n) = s.strip_prefix("type:") {
            return n.parse().ok().filter(|&t| t >= 0).map(Kind);
        }
        KINDS.iter().find(|(n, _)| n.eq_ignore_ascii_case(s)).map(|&(_, t)| Kind(t))
    }

    pub fn name(self) -> String {
        KINDS.iter().find(|&&(_, t)| t == self.0).map_or_else(|| format!("type:{}", self.0), |(n, _)| (*n).to_owned())
    }

    /// The centipede places itself (a random point on a ring of radius 30).
    pub fn takes_position(self) -> bool {
        !matches!(self.0, 2 | 5 | 9)
    }

    /// `World::SpawnNearPlayer` kind for skulls: 0 (1 HP), 2 (10 HP), 3 (100 HP).
    pub fn skull_kind(self) -> Option<u32> {
        matches!(self.0 - SKULL_TYPE, 0 | 2 | 3).then(|| (self.0 - SKULL_TYPE) as u32)
    }
}

/// Spawn point. `y` keeps the enemy's own height when left out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pos {
    pub x: f32,
    pub y: Option<f32>,
    pub z: f32,
}

/// One enemy of a wave: `<name>[@x,z | @x,y,z][:phase]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entry {
    pub kind: Kind,
    pub at: Option<Pos>,
    /// Boss only: phase steps it starts with, as if that many wave enemies
    /// had died after it spawned (0 = as the game spawns it).
    pub phase: u8,
}

/// The boss's phases: 0 dormant, 1 to 4 its transitions, 5 its last state.
pub const MAX_PHASE: u8 = 5;

impl Entry {
    pub fn of(kind: Kind) -> Self {
        Entry { kind, at: None, phase: 0 }
    }
}

fn parse_entry(tok: &str) -> Result<Entry, String> {
    // `type:<n>` has a colon of its own.
    let split = match tok.strip_prefix("type:") {
        Some(n) => 5 + n.find(['@', ':']).unwrap_or(n.len()),
        None => tok.find(['@', ':']).unwrap_or(tok.len()),
    };
    let (name, mut rest) = tok.split_at(split);
    let kind = Kind::from_name(name).ok_or_else(|| unknown_kind(name))?;
    let mut e = Entry::of(kind);
    while !rest.is_empty() {
        let end = rest[1..].find(['@', ':']).map_or(rest.len(), |i| i + 1);
        let (part, next) = rest.split_at(end);
        rest = next;
        if let Some(p) = part.strip_prefix('@') {
            if !kind.takes_position() {
                return Err(format!("`{name}` picks its own spawn point; it takes no position"));
            }
            let n: Vec<f32> = p.split(',').map(number).collect::<Result<_, _>>()?;
            e.at = Some(match n[..] {
                [x, z] => Pos { x, y: None, z },
                [x, y, z] => Pos { x, y: Some(y), z },
                _ => return Err(format!("`@{p}`: expected @x,z or @x,y,z")),
            });
        } else if let Some(p) = part.strip_prefix(':') {
            if kind != BOSS {
                return Err(format!("only `boss` has phases, not `{name}`"));
            }
            e.phase = p.parse().ok().filter(|&n| n <= MAX_PHASE).ok_or_else(|| format!("`:{p}`: phase is 0 to {MAX_PHASE}"))?;
        }
    }
    Ok(e)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PowerupRule {
    /// The game decides (gems collected since the last one, see the docs).
    Game,
    Off,
    /// Every `every` seconds of game clock, the first at `after`.
    Every { every: f32, after: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Powerup {
    pub rule: PowerupRule,
    /// Seconds it stays once it appeared (game: mercury 10, sulfur 5).
    pub lasts: Option<f32>,
}

impl Default for Powerup {
    fn default() -> Self {
        Powerup { rule: PowerupRule::Game, lasts: None }
    }
}

/// The two powerup slots of the game's "retrieval" object, in slot order.
pub const POWERUPS: [&str; 2] = ["mercury", "sulfur"];

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// Arena radius for runs (game: 48, the lobby is 15).
    pub arena_radius: Option<f32>,
    /// `false` holds the arena at full size (the game shrinks it by up to 22
    /// as gems are collected).
    pub arena_shrink: bool,
    /// New waves wait while this many enemies are alive (0: no cap).
    pub enemy_cap: u32,
    /// Also start the next wave every this many seconds (0: only on kills).
    pub wave_every: f32,
    pub powerups: [Powerup; 2],
    /// Replacement spawn table, one entry per wave. `None`: the game's.
    pub waves: Option<Vec<Vec<Entry>>>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            arena_radius: None,
            arena_shrink: true,
            enemy_cap: 0,
            wave_every: 0.0,
            powerups: Default::default(),
            waves: None,
        }
    }
}

impl Config {
    /// The table as the game stores it: each wave's types, then -1.
    pub fn table(&self) -> Option<Vec<i32>> {
        let waves = self.waves.as_ref()?;
        Some(waves.iter().flat_map(|w| w.iter().map(|e| e.kind.0).chain([-1])).collect())
    }
}

pub fn parse(src: &str) -> Result<Config, String> {
    let mut c = Config::default();
    for (n, raw) in src.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("");
        let toks: Vec<&str> = line.split_whitespace().collect();
        if toks.is_empty() {
            continue;
        }
        directive(&mut c, &toks).map_err(|e| format!("line {}: {e}", n + 1))?;
    }
    Ok(c)
}

fn directive(c: &mut Config, t: &[&str]) -> Result<(), String> {
    match t {
        ["arena", "radius", r] => c.arena_radius = Some(positive(r)?),
        ["arena", "shrink", v] => c.arena_shrink = on_off(v)?,
        ["enemy", "cap", n] => c.enemy_cap = n.parse().map_err(|_| format!("`{n}` is not a whole number"))?,
        ["wave", "every", s] => c.wave_every = seconds(s)?,
        ["wave", kinds @ ..] => {
            let wave = match kinds {
                [] | ["none"] => Vec::new(),
                _ => kinds.iter().map(|k| parse_entry(k)).collect::<Result<_, _>>()?,
            };
            c.waves.get_or_insert_with(Vec::new).push(wave);
        }
        ["powerup", name, rest @ ..] => {
            let slot = POWERUPS
                .iter()
                .position(|p| p.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("unknown powerup `{name}` (mercury, sulfur)"))?;
            let p = &mut c.powerups[slot];
            match rest {
                ["game"] => p.rule = PowerupRule::Game,
                ["off"] => p.rule = PowerupRule::Off,
                ["every", s] => {
                    let every = seconds(s)?;
                    p.rule = PowerupRule::Every { every, after: every };
                }
                ["every", s, "after", a] => p.rule = PowerupRule::Every { every: seconds(s)?, after: seconds(a)? },
                ["lasts", s] => p.lasts = Some(positive(s)?),
                _ => return Err("expected `powerup <name> game|off|every <s> [after <s>]|lasts <s>`".into()),
            }
            if matches!(p.rule, PowerupRule::Every { every, .. } if every <= 0.0) {
                return Err("`every` needs more than 0 seconds".into());
            }
        }
        _ => return Err(format!("unknown directive `{}`", t.join(" "))),
    }
    Ok(())
}

fn number(s: &str) -> Result<f32, String> {
    s.trim_end_matches('s').parse::<f32>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("`{s}` is not a number"))
}

fn seconds(s: &str) -> Result<f32, String> {
    number(s).and_then(|v| if v >= 0.0 { Ok(v) } else { Err(format!("`{s}` is negative")) })
}

fn positive(s: &str) -> Result<f32, String> {
    number(s).and_then(|v| if v > 0.0 { Ok(v) } else { Err(format!("`{s}` must be above 0")) })
}

fn on_off(s: &str) -> Result<bool, String> {
    match s {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(format!("expected on or off, found `{s}`")),
    }
}

fn unknown_kind(k: &str) -> String {
    let names: Vec<&str> = KINDS.iter().map(|(n, _)| *n).collect();
    format!("unknown enemy `{k}` (one of {}, or type:<n>)", names.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_changes_nothing() {
        let c = parse("# nothing\n\n").unwrap();
        assert_eq!(c, Config::default());
        assert!(c.arena_shrink && c.table().is_none());
    }

    #[test]
    fn parses_every_directive() {
        let c = parse(
            "arena radius 40\narena shrink off\nenemy cap 12\nwave every 30s\n\
             powerup mercury every 20 after 5\npowerup sulfur off\npowerup sulfur lasts 8\n\
             wave owl\nwave scuttles snake # comment\nwave\nwave owl_boss type:7\n",
        )
        .unwrap();
        assert_eq!(c.arena_radius, Some(40.0));
        assert!(!c.arena_shrink);
        assert_eq!((c.enemy_cap, c.wave_every), (12, 30.0));
        assert_eq!(c.powerups[0].rule, PowerupRule::Every { every: 20.0, after: 5.0 });
        assert_eq!(c.powerups[1], Powerup { rule: PowerupRule::Off, lasts: Some(8.0) });
        assert_eq!(c.table().unwrap(), vec![11, -1, 12, 10, -1, -1, 13, 7, -1]);
    }

    #[test]
    fn every_defaults_after_to_every() {
        let c = parse("powerup sulfur every 25").unwrap();
        assert_eq!(c.powerups[1].rule, PowerupRule::Every { every: 25.0, after: 25.0 });
    }

    #[test]
    fn errors_name_the_line() {
        assert_eq!(parse("arena radius 40\nwave owl dragon").unwrap_err().split(':').next(), Some("line 2"));
        assert!(parse("arena radius -3").is_err());
        assert!(parse("powerup gold off").is_err());
        assert!(parse("powerup mercury every 0").is_err());
        assert!(parse("enemy cap lots").is_err());
    }

    #[test]
    fn shipped_file_is_the_games_table() {
        let c = parse(include_str!("../data/waves.cfg")).unwrap();
        let waves = c.waves.as_ref().unwrap();
        assert_eq!(waves.len(), 48);
        assert_eq!(waves[0], vec![Entry::of(Kind(11))]);
        assert_eq!(waves[1], vec![Entry::of(Kind(12)), Entry::of(Kind(10))]);
        assert_eq!(waves[41], vec![Entry::of(Kind(3)), Entry::of(BOSS)]);
        assert!(waves[42..].iter().all(Vec::is_empty));
        // 98 entries, like the table `tools/re/spawntable.py` prints.
        assert_eq!(c.table().unwrap().len(), 98);
        assert_eq!(Config { waves: None, ..c }, Config::default());
    }

    #[test]
    fn parses_positions_and_phases() {
        let c = parse("wave owl@10,-20 boss:3@0,30,0 type:13:1 trilo@-5.5,4").unwrap();
        let w = &c.waves.unwrap()[0];
        assert_eq!(w[0], Entry { kind: Kind(11), at: Some(Pos { x: 10.0, y: None, z: -20.0 }), phase: 0 });
        assert_eq!(w[1], Entry { kind: BOSS, at: Some(Pos { x: 0.0, y: Some(30.0), z: 0.0 }), phase: 3 });
        assert_eq!(w[2], Entry { kind: BOSS, at: None, phase: 1 });
        assert_eq!(w[3].at, Some(Pos { x: -5.5, y: None, z: 4.0 }));
    }

    #[test]
    fn rejects_bad_entries() {
        assert!(parse("wave owl:2").is_err());
        assert!(parse("wave boss:6").is_err());
        assert!(parse("wave centipede@1,2").is_err());
        assert!(parse("wave owl@1").is_err());
        assert!(parse("wave owl@1,x").is_err());
        assert!(parse("wave skull2:1").is_err());
    }

    #[test]
    fn skulls_are_types_the_sequencer_skips() {
        let c = parse("wave skull skull2@0,-20 skull3\nwave owl").unwrap();
        assert_eq!(c.table().unwrap(), vec![100, 102, 103, -1, 11, -1]);
        let w = &c.waves.unwrap()[0];
        assert_eq!(w.iter().map(|e| e.kind.skull_kind()).collect::<Vec<_>>(), vec![Some(0), Some(2), Some(3)]);
        assert_eq!(Kind(11).skull_kind(), None);
    }

    #[test]
    fn names_round_trip() {
        for &(n, t) in KINDS {
            assert_eq!(Kind::from_name(n), Some(Kind(t)));
            assert_eq!(Kind(t).name(), n);
        }
        assert_eq!(Kind::from_name("Spawner"), Some(Kind(11)));
        assert_eq!(Kind::from_name("owl_boss"), Some(BOSS));
        assert_eq!(Kind(99).name(), "type:99");
    }
}
