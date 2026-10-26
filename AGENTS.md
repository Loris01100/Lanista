# AGENTS.md

This file provides guidance to AI coding agents (Codex, Claude Code…) when working with code in this repository.

## Project

Lanista is a gladiator-management game written in Rust (solo developer, Steam target, Windows + Linux/Steam Deck). Phases 0 to 6 bis are done (tags `phase-0` to `phase-4`, later tags pending); phase 5 bis (combat accuracy, stuns, body parts and cuts, lasting effects, staff market, club wealth) is still growing, and phase 7 (learning AI) has begun. Each finished phase's milestone is a test: phase 4 `a_whole_season_is_played_saved_and_reloaded`, phase 6 `three_fights_are_watched_together_one_of_them_a_melee_of_twenty`, phase 6 bis `three_seasons_where_a_club_becomes_a_rival_one_closes_and_one_is_founded` (all in `club_screen.rs`), phase 5 `three_seasons_live_without_the_player` (`state.rs`).

## Docs

All in French. Read only the parts the task needs:

- [docs/feuille_de_route/README.md](docs/feuille_de_route/README.md): the technical roadmap: stack, architecture, determinism rules, the list of phases and their milestones.
- `docs/feuille_de_route/phase-<n>.md` (`phase-5-bis.md`, `phase-6-bis.md`): what each phase holds and how it was built, the place to look for how an existing feature works.
- [docs/Gladiator_Manager_Plan_de_conception.md](docs/Gladiator_Manager_Plan_de_conception.md): gameplay rules (fighters, combat formulas, economy, colosseums, competitions).
- [docs/Formats_graphiques.md](docs/Formats_graphiques.md): sprite, arena and sound formats, Aseprite export settings.
- `crates/<crate>/AGENTS.md`: that crate's conventions and pitfalls.

## Source of truth

Before implementing a feature, read the roadmap's README and the file of the phase it belongs to. When behavior, architecture or data diverge from them, update that phase's file (or the README for architecture) in the same change, in French: what the feature does and the names to find it by, not a restatement of the code.

## Commands

```sh
cargo build                          # whole workspace
cargo run -p lanista                 # the game executable
cargo run --release -p tools --bin simulate -- --seed 42    # one duel, log and report
cargo run --release -p tools --bin batch -- --fights 10000  # win rate per class, average duration
cargo run --release -p tools --bin career -- --seed 7 --weeks 10  # one fighter's weeks
cargo run --release -p tools --bin temperaments -- --worlds 8     # careers by temperament and trait, clubs closed, in worlds without the player
cargo run --release -p tools --bin calibrate -- --fights 20000    # fit data/stat_model.ron on real duels
cargo run --release -p tools --bin compare -- --fights 20000      # real duels against the model, fails on a gap over --tolerance points
cargo test                           # all crates
cargo test -p lanista_core <name>    # single test (substring filter)
cargo bench -p lanista_core          # melee timings (criterion, benches/melee.rs)
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo machete                        # unused dependencies
cargo deny check                     # advisories, licenses, sources (deny.toml)
cargo llvm-cov --workspace --lcov --output-path lcov.info  # coverage for SonarQube Cloud, before a local scan
```

On Linux, linking `lanista` needs the ALSA headers (`libasound2-dev`) for macroquad's audio.

Python module `lanista_py` (PyO3, cdylib, no Rust tests): create `python/.venv` once (`py -m venv python/.venv`, then `pip install maturin gymnasium stable-baselines3 onnx tensorboard` inside it), activate it, then `maturin develop -m crates/lanista_py/Cargo.toml` and `python python/lanista_env.py` (checks the Gymnasium environment and plays duels in parallel); `python python/train.py <class> duel` trains a class (then `melee`), `--steps 4096 --envs 2 --check 20` as a quick check, and `python python/export.py --version N` exports the models.

CI ([.github/workflows/ci.yml](.github/workflows/ci.yml)) runs all of the above; clippy and tests run on both Windows and Linux so determinism breaks show up. Each push to `main` also runs the SonarQube Cloud analysis with coverage (`sonar` job, `sonar-project.properties`, `SONAR_TOKEN` repository secret; automatic analysis is off in SonarQube Cloud). Any clippy warning fails CI, and the `pedantic` group is on for the whole workspace (`[workspace.lints.clippy]` in the root `Cargo.toml`; each crate has `[lints] workspace = true`, which a new crate must add too). Fix a lint rather than silencing it, or use a local `#[expect(clippy::…, reason = "…")]`. Package metadata (edition, authors, repository, `license-file`, `publish = false`) lives in `[workspace.package]` and each crate inherits it with `<field>.workspace = true`; a new crate must do the same. `publish = false` is what lets `deny.toml` skip license checks for workspace crates. The code is proprietary (all rights reserved, see `LICENSE`).

`[profile.dev.package."*"] opt-level = 2` in the root `Cargo.toml` is deliberate: simulations are too slow with unoptimized dependencies in debug.

## Crate naming vs. the roadmap

The roadmap uses placeholder names; the actual crates are:

| Roadmap name | Crate | Role |
| --- | --- | --- |
| `gladiator_core` | `lanista_core` | Pure game logic (combat, AI, fighters, world, economy, save) |
| `moteur` | `reesch` | Generic engine (macroquad + egui: loop, sprites, input, audio, screens) |
| `gladiator` | `lanista` | Game binary; wires core + engine |
| `gladiator_py` | `lanista_py` | PyO3 module for RL training (the `Environment` class) |
| `gladiator_ia_onnx` | `lanista_ia_onnx` | ONNX inference with `tract`, for tests only; the game plays the learned AI in `lanista_core::learned` |
| `tools` | `tools` | Command-line binaries (`simulate`, `batch`, `career`, `calibrate`, `compare`, `temperaments`) |

The workspace picks up any `crates/*` automatically.

## Architecture rules (enforced by crate dependencies)

`cargo deny check` enforces the forbidden dependencies below through `[bans] deny` + `wrappers` in [deny.toml](deny.toml). A new crate that may legitimately depend on a banned crate must be added to its `wrappers`; `lanista_ia_onnx`, `lanista_py` and `tools` are listed there.

- `lanista_core` must never depend on macroquad, egui, tract or pyo3 — logic is runnable headless (tools, tests, Python training).
- `reesch` must never depend on `lanista_core`; it stays game-agnostic. Only add engine features a current game phase needs.
- Refer to entities by IDs (`FighterId(u32)`) stored in `Vec`/tables, not references.
- A week is a function (`next_week(&mut GameState, &Decisions)`), a combat step is a function (`tick(&mut Arena, &mut [Box<dyn CombatAi>])`). UI only builds decisions and reads state.
- Logic runs at a fixed tick rate; rendering interpolates. Speed-ups just run more ticks per frame.
- Design data (classes, spells, arenas) lives in RON files under `data/`, not in code. Saves are JSON, versioned with migrations from the Steam release on (see Validation).

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

Before handing off code changes, run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and the relevant `cargo test`; run `cargo machete` and `cargo deny check` too when dependencies changed. In Claude Code, hooks in `.claude/settings.json` run `cargo fmt --all` after each Rust file edit and the clippy command above before handing off (`.claude/hooks/`); the tests are not run by any hook. Saves are not versioned until the Steam release: a change to the saved types needs no new version nor migration (the `save-version` skill, `.claude/skills/save-version/SKILL.md`, is for after the release). Add dependencies with `cargo add`; roadmap versions are indicative. Prefer `try_from` over `as` for narrowing casts (pedantic flags `as`).

Tests: `proptest` for invariants (HP never above max, a combat always ends), `insta` snapshots for reference combats (a seed always produces the same log). Review snapshot changes with `cargo insta review` and commit the `.snap` files; CI sets `CI=true`, so a missing or changed snapshot fails.

## Git

- Never commit: the developer reviews and commits every change. After changing files, end the reply with a proposed commit message in a code block.
- Commit messages: lowercase type, a space before the colon, short summary: `feat : …`, `fix : …`, `chore : …`, `docs : …`, `test : …`.
- Never tag, push or publish without an explicit request.
- `Cargo.lock` is committed (binary project).
- `.githooks/pre-commit` rejects unformatted code; enable once per clone with `git config core.hooksPath .githooks`.
- Git LFS tracks `*.png`, `*.ogg`, `*.wav`, `*.aseprite`, `*.ttf`, `*.otf`; line endings normalized to LF.
- Git convention: `main` always working, commit directly to `main` until the Steam release, then one branch per feature, a tag per phase (`phase-0`, `phase-1`…).
