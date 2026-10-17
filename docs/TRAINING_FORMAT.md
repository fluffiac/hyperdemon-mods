# `.training` files

The `tutorials` mod loads every `.training` file in `<game>/trainings/` (custom
trainings) and `<game>/trainings/builtin/` (the game's own). Each
file describes one TRAINING scenario: its hint steps and their objectives,
what to keep spawned, and for custom trainings the clip and the menu row.
The 13 built-in trainings ship in the same format
(`mods/tutorials/game/trainings/builtin/`). Clip and thumbnail paths are
relative to the `.training` file.

```text
# Comments start with '#'.
training 0xF0000001 "TRIPLE STUN"
menu "TRIPLE STUN" "custom"
clip triple_stun.hdclip

step "-Three ENEMY 1s at once."
step "-Finish one with a DASH: tap {jump} into a STUNNED enemy." until scuttles_boost >= 3

keep 3 spawner every 1
```

One directive per line. Strings are double-quoted (`\"` and `\\` escape).
Numbers may carry an `s` suffix (`every 0.5s`). Errors are logged to
`mods/loader.log` with the file and line; a file with an error is skipped.

## Directives

| Directive | Meaning |
|---|---|
| `training <id> "<title>"` | First line. `<id>` is the clip id, decimal or `0x` hex (custom trainings: stay in `0xF0000000..0xFFFFFFFF`). `<title>` is the large text above the hints; it may be `""`. |
| `builtin` | One of the game's own trainings: the server lists it and serves its clip. No `menu`/`clip`. |
| `menu "<title>" ["<subtitle>" [<seconds>]]` | Custom only: the row in the TRAINING menu. Defaults: the HUD title, `"custom"`, 0. |
| `clip <path>` | Custom only: the starting situation, a clip saved with the in-game clip editor (`%APPDATA%\HyperDemon\clips\<name>_<id>.hdclip`). Relative to the file. Without it, or while the file is missing, the training plays on a built-in clip (see below). |
| `thumbnail <path>` | Custom only: image for the menu row. |
| `step "<text>" [until <expr> >= <n>] [when playing]` | A hint line, at most 5. See below. |
| `keep <n> <kind> [at <x> <y> <z>] [every <seconds>] [after step <k>]` | Spawn rule. See below. |
| `show score` | Draw the big score above the hints (the score training). |
| `drain score` | The score drops by 1 per second while the player is alive. |

## Steps

Steps are shown in order under the title. The current one is brighter.
A step with `until` is an objective: it is done once the expression reaches
the target, and the game shows an `n/N` counter left of it once the player
has taken over. A step without `until` is information and passes as soon as
it is reached. When every step is done the training is complete: the run
ends, and the game marks the clip id completed in the menu and saves.

A training with no objective at all never completes; it is a sandbox.

While the demo plays (before TRY IT), an objective passes as soon as its
expression is above 0, so the hints advance along with the recording.

`when playing` hides the step until the player has taken over.

Counters of steps not reached yet are held at 0: progress only counts from
the moment its step becomes current.

`{bind}` in the text prints the key bound to an action, the way the game
prints `SPACE` or `LMB`: `{forward}`, `{back}`, `{left}`, `{right}`,
`{fire}` (daggers), `{laser}` (laser/grab), `{jump}` (jump/dash). `{{` and
`}}` print braces.

### Expressions

```text
expr := term { "+" term }
term := atom [ ">" <n> ]          # 1 if atom > n, else 0
atom := <counter> | score | <n> | "(" expr ")"
```

`score` is the run score (truncated). `(bomb1_lasered > 0) + (bomb2_lasered > 0) >= 2`
counts how many of two things happened at least once.

### Counters

Counters live in the World and are incremented by the game during play.
Names come from the built-in hint that displays them; what exactly each
counts is inferred from that text.

| Counter | Built-in step |
|---|---|
| `bunny_hop` | BUNNY HOP+FAKE: tap jump as you reach the ground |
| `fake` | BUNNY HOP+FAKE: change direction right before jumping |
| `dodge` | DODGE: dodge when an enemy's eyes flash |
| `dagger_jump` | DAGGER JUMP+STOMP: shoot the ground to dagger jump |
| `air_dagger_jump` | DAGGER JUMP+STOMP: repeat in the air |
| `stomp` | DAGGER JUMP+STOMP: stomp |
| `dash` | DASH+SLIDE: dash in the air |
| `slide` | DASH+SLIDE: hold to slide |
| `slide_jump` | DASH+SLIDE: jump out of a slide |
| `rapid_fire` | RAPID+SHOT: daggers fired holding fire |
| `shot` | RAPID+SHOT: tapped shots |
| `gems` | RAPID+SHOT: gems collected |
| `laser_charge` | LASER+RICOCHET: hold laser to suck in gems |
| `laser` | LASER+RICOCHET: lasers shot |
| `laser_lock_on` | LASER+RICOCHET: ricochet lock-ons |
| `grab` | GRAB+THROW: grab a bomb |
| `drop` | GRAB+THROW: drop it |
| `throw` | GRAB+THROW: throw it |
| `bomb1_lasered`, `bomb2_lasered` | GRAB+THROW: laser each bomb |
| `bluepowerup` | POWERUPS: touch the powerup |
| `bluepowerup_laser` | POWERUPS: collect it as a laser attack |
| `bluepowerup_air` | POWERUPS: collect it in the air above the snake |
| `spawner_dagger_kill` | SPAWNER (ENEMY 1): defeat with daggers |
| `spawner_gem_laser` | SPAWNER (ENEMY 1): laser the big gem |
| `spawner_gem_dash` | SPAWNER (ENEMY 1): gem dash |
| `scuttles_stun` | SCUTTLES (ENEMY 2): stun it |
| `scuttles_attack` | SCUTTLES (ENEMY 2): get attacked while it is stunned |
| `scuttles_stunned` | SCUTTLES (ENEMY 2): keep it stunned / finish it |
| `scuttles_boost` | SCUTTLES (ENEMY 2): dash or jump into a stunned enemy |
| `snakes_attention` | SNAKES (ENEMY 3): get its attention |
| `snakes_spin` | SNAKES (ENEMY 3): strike then spin |
| `snakes_head_hit` | SNAKES (ENEMY 3): hit the heads |
| `snakes_kill` | SNAKES (ENEMY 3): defeat it |
| `snakes_stomp_stun` | SNAKES (ENEMY 3): stomp near the heads |
| `spiders_hide` | SPIDERS (ENEMY 4): it hides |
| `spiders_drop` | SPIDERS (ENEMY 4): make it drop |
| `spiders_dropped_damage` | SPIDERS (ENEMY 4): damage it while dropped |

Not yet verified: whether the game increments these counters while a custom
training is running, or only for the matching built-in one.

## Spawns

`keep <n> <kind>` keeps `n` of a kind alive while the player is in control.
When there are fewer, it waits `every` seconds (default 1, `every 0` for
none), spawns one, and waits again before the next. `after step <k>` only
applies once steps 1 to k are done.

| Kind | What | `at` |
|---|---|---|
| `spawner`, `scuttles`, `snakes`, `spiders` | The enemies of the ENEMY 1–4 trainings (`spawner` is ENEMY 1, `scuttles` is ENEMY 2, `snakes` is ENEMY 3, `spiders` is ENEMY 4; POWERUPS calls it the SNAKE) | no |
| `skull3` | The DODGE training's enemy | offset from the player, default `0 0 -35` |
| `gem` | Practice target gem | world position, default `0 10 -20` |
| `bomb` | GRAB+THROW bomb | world position, default `-10 10 -20` |
| `bluepowerup` | The POWERUP (only one exists; `keep 1 bluepowerup`) | no |

## Trainings without a clip

A custom training with no `clip`, or whose clip file is missing, still shows
up in the menu and plays on a stand-in clip:

* `trainings/stand_in.hdclip` if it exists. Record it once with the clip
  editor: about a second of standing still with nothing spawning, and the
  clip start at the beginning. Then copy it from
  `%APPDATA%\HyperDemon\clips\` and rename it.
* Otherwise, the BUNNY HOP+FAKE clip from the game's server (a movement drill
  with no enemies).

TRY IT starts from the stand-in's takeover point. Use it to draft steps and
spawns before recording a real clip.

While that stand-in demo plays, the mod hides it: the screen is black
(the hints and the TRY IT button stay visible; the input display and scrub
bar are hidden), all sound is muted, and
every counter the training's objectives read is held at 0, so nothing the
recording does counts. Steps without an objective still advance. Sound and
counters come back when the player takes over.

The mod can't generate an empty clip instead: the clip body is not decoded,
and a hand-made zero-tick clip hangs the game (its playback divides the clip
time by the clip length).

## Built-in vs custom

Every training with a file is run by the mod alone, built-in ones included:
while the game's hardcoded per-training code runs, the mod shows it a
neutral clip id, so the game draws, counts and spawns nothing on its own.
Editing a built-in's file therefore changes that built-in (its clip and menu
row still come from the server). Delete the file to get the game's own
behaviour back for that training.
