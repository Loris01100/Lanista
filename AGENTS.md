# AGENTS.md

This file provides guidance to AI coding agents (Codex, Claude Code…) when working with code in this repository.

## Project

Lanista is a gladiator-management game written in Rust (solo developer, Steam target, Windows + Linux/Steam Deck). The technical roadmap is [docs/Gladiator_Manager_Feuille_de_route_Rust.md](docs/Gladiator_Manager_Feuille_de_route_Rust.md) (in French); it is the reference for stack, architecture, phases and determinism rules. Gameplay rules (fighters, combat formulas, economy, colosseums, competitions) live in the design plan [docs/Gladiator_Manager_Plan_de_conception.md](docs/Gladiator_Manager_Plan_de_conception.md) (in French). Sprite, arena and sound formats and the Aseprite export settings are in [docs/Formats_graphiques.md](docs/Formats_graphiques.md) (in French). Phases 0 to 4 are done (tags `phase-0` to `phase-4`); phase 5 (the world) is done, its milestone checked by `three_seasons_live_without_the_player` in `state.rs` (rival deaths too high for off-season releases to happen, rival money swelling: taken up in phase 5 bis, with staff hired on the market by the player and the rival clubs), awaiting the `phase-5` tag. Phase 5: `lanista_core::colosseum` and `data/world.ron` hold the colosseums (two: Capoue, free, and Rome; arena type without combat effect until phase 6, prestige making the fighters generated for them stronger, purses, Elo start and K); `GameState::colosseums` keeps each one's Elo ranking (melee-ready: `record_places`, `log::places`; those who stop fighting leave it) and its champion, the fighters themselves living in the clubs (`Contender::Club`, `Contender::Rival(RivalId)`, `Rosters`): the title changes hands only in a title fight (`Activity::TitleFight`) and must be defended every 4 weeks (rival champions defend against the best other rival fighter at the end of the week, a club champion past its deadline loses the title; a survived defense pays the champion bonus) and the rival fighters' challenges (`Activity::Challenge(ColosseumId, RivalId)`: one the club sends is accepted if Elo gives the rival fighter its club's `accept_percent`; each week a fit, ranked fighter may receive one, kept in `GameState::challenges` for a week; refusing it without fighting costs fame and Elo); `lanista_core::rivals` holds the rival clubs (three in `data/world.ron`, profiles cautious, aggressive and merchant setting their thresholds; `GameState::rivals` keeps each club's money and roster, a fighter who leaves keeping its entry; the colosseums' first residents are shared out in turn at the opening; each week `GameState::rival_plans` rests, cares, trains or registers each fighter in the colosseum where its Elo chances are best, the club's fighters meet registered rival fighters, and `rival_weeks` pairs the others by Elo for real duels; a club below `roster.wanted` signs the best free agent, or first tries to buy a better fighter from another club when no free agent is worth its weakest (`rival_signings`), a club at `roster.max` buys no one); `lanista_core::market` holds the market (rules in `data/economy.ron`; `market::value` from stats, fame and record (`CareerFighter::record`); `GameState::free_agents`, signed for nothing, topped up each week, rookies and the release of old rival fighters worth less than their club's average each off-season; `Decisions::moves` holds `Move`s: observe, sign, release, put on or off the transfer list (`GameState::transfer_list`; rival clubs with a place make `SaleOffer`s, accepted the next week, the fighter fighting on until then), and offer a price for a rival fighter, answered at the end of the week by its club (accept, `GameState::counter_offer` or refuse) then by the fighter (`market::refuses_to_join`); `WeekSummary::market` tells what happened; a fighter who leaves the club keeps its entry with `Status::Sold` or `Status::Released`; stats out of the club are hidden or seen as ranges, revealed week after week of observation (`market::stat_ranges`, `CareerFighter::observed_weeks`)), shown in the club screen's "Marché" view; `lanista_core::staff` holds the staff: a trainer, a doctor and a recruiter (`Role`), each with levels 1 to 5 in `data/economy.ron` and a weekly salary paid even into debt (`Flow::Salary`, a `Transaction` with no fighter); the trainer sets the session price and its `Coaching` raises the training progression chance and spares form, the doctor's `Medicine` makes care keep fewer weeks at its price and may spare an after-effect (`injury::spare_after_effect`; `CareerFighter::play_week` takes the `StaffEffects`), the recruiter's `Scouting` reveals more stats a week, narrows the ranges, estimates the potential (`market::potential_range`) and sees the personality (`market::personality_seen`); `GameState::staff`, changed by `Decisions::staff` (`StaffMove`s, the first for each post counting), shown in the club screen's "Personnel" view; the club screen's colosseums, market and staff views live in `club_screen/` (`colosseums.rs`, `market.rs`, `staff.rs`), with the sheet of a fighter out of the club (`View::Outside`: what the club sees, its place in each colosseum, market actions) and a rival club's page (`View::Rival`), opened by clicking names; rival clubs have none yet; a fight plan names its colosseum (`Activity::Fight(ColosseumId)`), and the club screen has a "Colisées" view; `lanista_core::stat_model` resolves fights between rival fighters (`rival_weeks` and title defenses, through `CareerFighter::modelled_fight` and `modelled_fame_and_death`; the club's fights stay real duels): a logistic regression on the winner from stats, fear, surrender threshold, traits and class, then tables for the time limit, the loser's death by determination and spectacular hits, all read from `data/stat_model.ron`, which the `calibrate` tool (`tools::calibration`) writes from thousands of real duels; rerun it after any balancing change, then check it with `compare` (`tools::comparison`: the same fights as real duels through `CareerFighter::dueled_fight` and through the model, time limit, stronger, class, death and injury rates side by side; the first fighter's win rate is shown unchecked, the duel engine favouring `FighterId(0)`, who casts first in a shared tick). Phase 4 (management): `lanista_core::state` holds `GameState` (week, the club's fighters, the game's one `ChaCha8Rng`), `Decisions` (one `WeekPlan` per fighter, `suggested_plan` when missing) and `next_week`, used by the career screen and the `career` tool; `lanista_core::calendar` derives the season, week of season and phase (`Date::of`, `Date::phase`) from the week count, with the 30-week season (22 competition, 4 great event, 4 off-season) in `data/calendar.ron`, a fighter's birthday coming each season; `lanista_core::economy` and `GameState::money` handle money (`data/economy.ron`: upkeep, training, staff; each colosseum's fee, prize and receipts are in `data/world.ron`; `next_week` returns a `WeekSummary` with each move in a ledger, drops choices the club cannot pay, and a free agent is signed (`Move::Sign`) only while `GameState::can_hire`: at most `max_fighters` (3) fighters in activity); `lanista_core::save` writes the `GameState` (random generator included) as versioned JSON (`to_json`, `from_json` with one JSON migration per version in `MIGRATIONS`; bump `VERSION` and add a migration with any change to the saved types); `tests/saves.rs` checks reloads with `proptest` and loads every kept save in `crates/lanista_core/tests/saves/v<N>.json` (one per version; when bumping `VERSION`, keep the old files and write the new one with `cargo test -p lanista_core --test saves -- --ignored`; the current file must be rewritten byte for byte, so a saved-type change without a new version fails). Its milestone is checked by `a_whole_season_is_played_saved_and_reloaded` in `club_screen.rs`. `lanista` opens on the club of seed 7 (`club_screen.rs`, French texts in `texts.rs`: one screen owning the `GameState`, whose views are the general menu with alerts, the week's market moves and "Semaine suivante" on Space, the fighter list deciding each week in one click, a fighter's sheet with its history, and the fight summary from `FightSummary::report`; a week's fights are watched one after another with `CombatScreen::watch` from `FightSummary::start`; its tests check ten weeks on screen match the `career` tool; the game is saved after every week to `autosave.json` and on demand to `save.json` in the platform data folder from `directories` (`saves.rs`), resumed from the autosave at launch), and its combat screen also shows the standard duel of a seed picked on screen (`combat_screen.rs`: interpolated positions, animated placeholder sprites from `assets/placeholder/fighter.png`, x0.5/x1/x1.5 speed and skip to the result, placeholder hit/death sounds and music with volume sliders; a dead fighter's blood stain spreads under the darkened body; its tests check the watched fight matches `simulate` for the same seed); `lanista_py` and `lanista_ia_onnx` still hold only the phase 0 prototypes (PyO3 `addition`, `tract` ONNX inference); `lanista_core` holds the phase 1 duel engine (spells, utility AI, fight log, `insta` and `proptest` tests), `fighter::form`, `fighter::injury`, `fighter::illness`, `fighter::care`, `fighter::personality`, `fighter::morale`, `fighter::fame`, `fighter::progression`, `fighter::retirement` and `fighter::career` (phase 3: form 0 to 100, its weekly changes and the injury/illness risk formula; injuries with location, severity, dominant hand, healing and permanent after-effects; illnesses on three levels with stat malus, slower recovery and worsening; player-decided care that shortens recovery for a price; personality, whose determination sets the surrender threshold: in combat a fighter gives up alive at `surrender_hp` and dies at 0 HP; morale 0 to 100 with a small combat bonus/malus; fame 0 to 100 from fight outcomes; level-free progression capped by a hidden potential, paced by age and morale, yearly decline from 40; retirement decided by the fighter from age 30; values in `data/fighter.ron`; the combat `Fighter` carries a `Condition` the utility AI reads: fear makes a hurt fighter back away, aggression presses, showmanship waits for the big spell, all neutral by default; `CareerFighter` holds it all from week to week and `play_week` plays one week of care, rest, training or a fight, healing and ageing) and `duel`, the standard duel setup from a seed shared by the game, the tools and the tests; `tools` holds the `simulate`, `batch`, `career`, `calibrate` and `compare` binaries, and `reesch` holds the phase 2 engine (`FixedStep`, the fixed-rate loop; `ScreenStack`, the screen stack; `SpriteSheet`/`Animation`/`AnimationPlayer`, spritesheet animations with markers; `fit_camera`, a 2D camera fitting a world area to any screen; `Bindings`, game actions bound to keys and mouse buttons, ignoring input egui uses; `Assets`, textures/sounds/fonts keyed by path under `assets/`; `Audio`, sound effects and one looping music track with separate volumes, passed to `Screen::update`; `run`, the main loop with egui, scaling the UI to the window height (designed for 800 px), the egui theme being `lanista`'s `theme.rs`; `reesch::egui` is the only egui the game uses).

## Source of truth

Read the roadmap before implementing a feature. When behavior, architecture or data diverge from it, update it in the same change, in French.

## Commands

```sh
cargo build                          # whole workspace
cargo run -p lanista                 # the game executable
cargo run --release -p tools --bin simulate -- --seed 42    # one duel, log and report
cargo run --release -p tools --bin batch -- --fights 10000  # win rate per class, average duration
cargo run --release -p tools --bin career -- --seed 7 --weeks 10  # one fighter's weeks
cargo run --release -p tools --bin calibrate -- --fights 20000    # fit data/stat_model.ron on real duels
cargo run --release -p tools --bin compare -- --fights 20000      # real duels against the model, fails on a gap over --tolerance points
cargo test                           # all crates
cargo test -p lanista_core <name>    # single test (substring filter)
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo machete                        # unused dependencies
cargo deny check                     # advisories, licenses, sources (deny.toml)
cargo llvm-cov --workspace --lcov --output-path lcov.info  # coverage for SonarQube Cloud, before a local scan
```

On Linux, linking `lanista` needs the ALSA headers (`libasound2-dev`) for macroquad's audio.

Python module `lanista_py` (PyO3, cdylib, no Rust tests): create `python/.venv` once (`py -m venv python/.venv`, then `pip install maturin` inside it), activate it, then `maturin develop -m crates/lanista_py/Cargo.toml` and `python -c "import lanista_py"`.

CI ([.github/workflows/ci.yml](.github/workflows/ci.yml)) runs all of the above; clippy and tests run on both Windows and Linux so determinism breaks show up. Each push to `main` also runs the SonarQube Cloud analysis with coverage (`sonar` job, `sonar-project.properties`, `SONAR_TOKEN` repository secret; automatic analysis is off in SonarQube Cloud). Any clippy warning fails CI, and the `pedantic` group is on for the whole workspace (`[workspace.lints.clippy]` in the root `Cargo.toml`; each crate has `[lints] workspace = true`, which a new crate must add too). Fix a lint rather than silencing it, or use a local `#[expect(clippy::…, reason = "…")]`. Package metadata (edition, authors, repository, `license-file`, `publish = false`) lives in `[workspace.package]` and each crate inherits it with `<field>.workspace = true`; a new crate must do the same. `publish = false` is what lets `deny.toml` skip license checks for workspace crates. The code is proprietary (all rights reserved, see `LICENSE`).

`[profile.dev.package."*"] opt-level = 2` in the root `Cargo.toml` is deliberate: simulations are too slow with unoptimized dependencies in debug.

## Crate naming vs. the roadmap

The roadmap uses placeholder names; the actual crates are:

| Roadmap name | Crate | Role |
| --- | --- | --- |
| `gladiator_core` | `lanista_core` | Pure game logic (combat, AI, fighters, world, economy, save) |
| `moteur` | `reesch` | Generic engine (macroquad + egui: loop, sprites, input, audio, screens) |
| `gladiator` | `lanista` | Game binary; wires core + engine |
| `gladiator_py` | `lanista_py` | PyO3 module for RL training (phase 0 prototype for now) |
| `gladiator_ia_onnx` | `lanista_ia_onnx` | ONNX inference of the learned AI with `tract` (phase 0 prototype for now) |
| `tools` | `tools` | Command-line binaries (`simulate`, `batch`, `career`, `calibrate`, `compare`) |

The workspace picks up any `crates/*` automatically.

## Architecture rules (enforced by crate dependencies)

`cargo deny check` enforces the forbidden dependencies below through `[bans] deny` + `wrappers` in [deny.toml](deny.toml). A new crate that may legitimately depend on a banned crate must be added to its `wrappers`; `lanista_ia_onnx`, `lanista_py` and `tools` are listed there.

- `lanista_core` must never depend on macroquad, egui, tract or pyo3 — logic is runnable headless (tools, tests, Python training).
- `reesch` must never depend on `lanista_core`; it stays game-agnostic. Only add engine features a current game phase needs.
- Refer to entities by IDs (`FighterId(u32)`) stored in `Vec`/tables, not references.
- A week is a function (`next_week(&mut GameState, &Decisions)`), a combat step is a function (`tick(&mut Arena, &mut [Box<dyn CombatAi>])`). UI only builds decisions and reads state.
- Logic runs at a fixed tick rate; rendering interpolates. Speed-ups just run more ticks per frame.
- Design data (classes, spells, arenas) lives in RON files under `data/`, not in code. Saves are versioned JSON with migrations.

## Naming

All code is in English: modules, types, functions, variables, constants, test names, code file names, comments. Only player-facing text and the `docs/` documents stay in French. The design plan's French stat abbreviations map to `hp` (PV), `melee_attack` (AC), `ranged_attack` (AL), `speed` (VIT), `melee_defense` (DC), `ranged_defense` (DL).

## Determinism (in `lanista_core`)

Same seed must replay the same combat across Windows and Linux (cloud saves, replays). [crates/lanista_core/clippy.toml](crates/lanista_core/clippy.toml) enforces these with `disallowed-types`/`disallowed-methods`; it applies to that crate only, and replaces any root `clippy.toml` for it.

- No `HashMap`/`HashSet` — use `BTreeMap` or sorted `Vec`.
- Float math (`sin`, `sqrt`, …) via `libm`, not `f32`/`f64` methods.
- One `ChaCha8Rng` per combat seeded explicitly; never `thread_rng()`.
- No wall-clock time (`Instant::now`); logic only knows ticks.
- Parallelize across independent combats (rayon), never inside one.

## Validation

Before handing off code changes, run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and the relevant `cargo test`; run `cargo machete` and `cargo deny check` too when dependencies changed. Add dependencies with `cargo add`; roadmap versions are indicative. Prefer `try_from` over `as` for narrowing casts (pedantic flags `as`).

Tests: `proptest` for invariants (HP never above max, a combat always ends), `insta` snapshots for reference combats (a seed always produces the same log). Review snapshot changes with `cargo insta review` and commit the `.snap` files; CI sets `CI=true`, so a missing or changed snapshot fails.

## Git

- Never commit: the developer reviews and commits every change. After changing files, end the reply with a proposed commit message in a code block.
- Commit messages: lowercase type, a space before the colon, short summary: `feat : …`, `fix : …`, `chore : …`, `docs : …`, `test : …`.
- Never tag, push or publish without an explicit request.
- `Cargo.lock` is committed (binary project).
- `.githooks/pre-commit` rejects unformatted code; enable once per clone with `git config core.hooksPath .githooks`.
- Git LFS tracks `*.png`, `*.ogg`, `*.wav`, `*.aseprite`, `*.ttf`, `*.otf`; line endings normalized to LF.
- Git convention: `main` always working, commit directly to `main` until the Steam release, then one branch per feature, a tag per phase (`phase-0`, `phase-1`…).
