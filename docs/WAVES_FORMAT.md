# `waves.cfg`

The `waves` mod reads `<game>/mods/waves/waves.cfg` and changes how a run
spawns: the table of waves, an enemy cap, a wave timer, the arena, and the
powerups. `cargo xtask install` ships the file with the game's own table
written out, so the game plays the same until you edit it. Your edits survive
a reinstall.

The file reloads when it is saved, or with **F8**. A change to the table
applies from the next run; every other setting applies at once. **F7** shows
a panel in the run with the arena radius, waves started, enemies alive, held
waves, and the powerup timers. Errors are shown there and logged to
`mods/loader.log` with the line. A file with an error is ignored, and the
previous rules stay in force.

The mod only acts in a live run. The lobby, replays, clips and trainings are
left alone, and the loader keeps blocking leaderboard uploads as for any mod.

```text
arena radius 40
arena shrink off
enemy cap 10
wave every 45
powerup mercury every 20 after 10
powerup sulfur off

wave owl@0,-20
wave trilo@15,0 snake@-15,0
wave spider spider
wave boss:3@0,30,0
```

One directive per line; `#` starts a comment. A number may carry an `s`
suffix (`45s`). Leave a directive out to keep the game's behaviour.

## How the game spawns a run

When you pick up the dagger, the game loads its spawn table: a list of waves,
each a group of enemies. The first wave (one owl) starts at once. After
that, **every enemy from a wave that dies starts the next wave in the table**,
and its enemies spawn together, each at a free spot within 43 of the arena
centre. Killing the first owl therefore brings the trilo and the snake
(wave 2). When the table runs out, nothing more spawns. The game's table has
48 waves; the last six are empty.

The game places the first owl of a run at 0,-20, the boss at 0,30,0 (high
above the centre), and a centipede on a random point of a ring of radius 30.

## Directives

| Directive | Meaning |
|---|---|
| `wave <enemy> [<enemy> ...]` | One wave, in order. The first `wave` line replaces the whole table. `wave none` (or a bare `wave`) is an empty wave: a kill that reaches it starts nothing. Each enemy may carry a spawn point and the boss a phase (below). |
| `wave every <s>` | Also start the next wave every `s` seconds of the run, as if a wave enemy had died at the arena centre. 0 = off (default). |
| `enemy cap <n>` | While `n` or more enemies are alive, waves that have not spawned yet wait. A whole wave then spawns at once, so the count can go past `n` by the size of one wave. 0 = no cap (default). Owls, trilos, snakes and spiders count; centipedes do not (their count is not known). |
| `arena radius <r>` | Arena radius in runs. The game uses 48 (the lobby is 15). |
| `arena shrink on\|off` | `off` keeps the arena at full size. The game shrinks it by up to 22 as gems are collected (fully at 149). Default `on`. |
| `powerup <name> game` | The game decides when it comes out (default; see below). `<name>` is `mercury` or `sulfur`. |
| `powerup <name> off` | It never comes out. |
| `powerup <name> every <s> [after <s>]` | It comes out every `s` seconds of the run, first at `after` (default: `s`), whenever the last one is gone. Like the game, it gives 3 s of warning. |
| `powerup <name> lasts <s>` | How long it stays once out. Game: mercury 10, sulfur 5. |

The game brings mercury out after 15 s once you have collected more than 9
gems, and sulfur after 30 s once you have more than 18. Later ones need more
gems (a quarter and two thirds more, roughly).

## Enemies

| Name | Also | Game's use |
|---|---|---|
| `owl` | `spawner` | ENEMY 1, wave 1 and often after |
| `trilo` | `scuttles` | ENEMY 2 |
| `snake` | `snakes` | ENEMY 3 |
| `spider` | `spiders` | ENEMY 4 |
| `centipede` | | wave 19 only |
| `boss` | `owl_boss` | the owl boss: wave 42, the game's last enemy |
| `trilo_boss`, `snake_boss`, `spider_boss`, `centipede_boss`, `centipede_2` | | not used by the game |
| `skull` | | a 1-HP skull |
| `skull2` | | a 10-HP skull (the DODGE training's) |
| `skull3` | | a 100-HP skull |
| `type:<n>` | | any raw table type |

`boss` is the owl's second variant. The other `*_boss` names are the other
enemies' second variants, which the game never uses. That they are those
enemies' bosses is a guess: the game has separate boss systems, and how they
spawn was not traced. Try them in a test run (`wave snake_boss` as the first
line) before building a table around them. `centipede_2` is the centipede's
third variant. Types that spawn nothing in the sequencer: 0, 1, 4, 6, 7.

**Skulls** are not part of the game's table: in a run the owls spawn them.
The mod spawns a wave's skulls itself, right after the rest of that wave
has spawned (so the enemy cap holds them too). Without a spawn point they
appear on a ring of radius 25 at height 10; with `@x,z` at height 10, or
`@x,y,z`. Killing them does not start the next wave: only the game's wave
enemies do that, so a wave of only skulls needs `wave every` or kills from
another wave to move on. They count toward nothing in the panel or the cap.
The game adds up to 1 unit of random jitter to a skull's spawn point.

## Spawn points and the boss's phase

An enemy in a `wave` line can carry options, written without spaces:

| Form | Meaning |
|---|---|
| `owl@10,-20` | spawn at x 10, z -20, at the enemy's own height |
| `boss@0,30,0` | spawn at x 0, height 30, z 0 |
| `boss:3` | the boss starts at phase 3 |
| `boss:2@0,-15` | both, in either order |

Coordinates are arena units with the centre at 0,0; the edge is at the arena
radius (48 by default, less once it shrinks). Nothing stops a point outside
the arena. The centipede places itself and takes no position. An enemy
without options spawns as the game decides.

**Phases.** The boss spawns dormant (phase 0). Every wave enemy that dies
afterwards advances it: the first wakes it (phase 1), the next three each
start a new phase (2 to 4), and the one after that puts it in its last
state (phase 5). `boss:N` runs the game's own phase step N times as the boss
spawns, so it starts as if N wave enemies had already died, and later kills
continue from there. Other bosses alive at that moment are not advanced.

## Limits

* The game's own progression stays: waves still only come from kills (or
  `wave every`). The mod changes what each wave holds, not the rule.
* The enemy cap holds whole waves, not single enemies.
* Replays of modded runs play back fine: the game records every spawn as it
  happens (spawn points included), and the mod reports the powerup times it
  sets. A boss started at a later phase is the exception: playback shows it
  starting dormant, because the extra phase steps are not recorded.
* Everything here comes from static analysis of build `7612dd72…`
  ([REVERSE_ENGINEERING.md](REVERSE_ENGINEERING.md), *Run spawner*). The
  mod has not been tested in game yet.
