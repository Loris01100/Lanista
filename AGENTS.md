# AGENTS.md

This file provides guidance to AI coding agents (Codex, Claude Code…) when working with code in this repository.

## Project

Lanista is a gladiator-management game written in Rust (solo developer, Steam target, Windows + Linux/Steam Deck). The technical roadmap is [docs/Gladiator_Manager_Feuille_de_route_Rust.md](docs/Gladiator_Manager_Feuille_de_route_Rust.md) (in French); it is the reference for stack, architecture, phases and determinism rules. Gameplay rules (fighters, combat formulas, economy, colosseums, competitions) live in the design plan [docs/Gladiator_Manager_Plan_de_conception.md](docs/Gladiator_Manager_Plan_de_conception.md) (in French). Phases 0 and 1 are done (tags `phase-0`, `phase-1`); next is phase 2 (the `reesch` engine). `lanista`, `lanista_py` and `lanista_ia_onnx` still hold only the phase 0 prototypes (sprite + egui button screen in `lanista`, run by `reesch`; PyO3 `addition` in `lanista_py`, `tract` ONNX inference in `lanista_ia_onnx`); `lanista_core` holds the phase 1 duel engine (spells, utility AI, fight log, `insta` and `proptest` tests), `tools` holds the `simulate` and `batch` binaries, and `reesch` holds the start of phase 2 (`FixedStep`, the fixed-rate loop; `ScreenStack`, the screen stack; `SpriteSheet`/`Animation`/`AnimationPlayer`, spritesheet animations with markers; `fit_camera`, a 2D camera fitting a world area to any screen; `Assets`, textures/sounds/fonts keyed by path under `assets/`; `run`, the main loop with egui; `reesch::egui` is the only egui the game uses).

## Source of truth

Read the roadmap before implementing a feature. When behavior, architecture or data diverge from it, update it in the same change, in French.

## Commands

```sh
cargo build                          # whole workspace
cargo run -p lanista                 # the game executable
cargo run --release -p tools --bin simulate -- --seed 42    # one duel, log and report
cargo run --release -p tools --bin batch -- --fights 10000  # win rate per class, average duration
cargo test                           # all crates
cargo test -p lanista_core <name>    # single test (substring filter)
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo machete                        # unused dependencies
cargo deny check                     # advisories, licenses, sources (deny.toml)
cargo llvm-cov --workspace --lcov --output-path lcov.info  # coverage for SonarQube Cloud, before the scan
```

On Linux, linking `lanista` needs the ALSA headers (`libasound2-dev`) for macroquad's audio.

Python module `lanista_py` (PyO3, cdylib, no Rust tests): create `python/.venv` once (`py -m venv python/.venv`, then `pip install maturin` inside it), activate it, then `maturin develop -m crates/lanista_py/Cargo.toml` and `python -c "import lanista_py"`.

CI ([.github/workflows/ci.yml](.github/workflows/ci.yml)) runs all of the above; clippy and tests run on both Windows and Linux so determinism breaks show up. Any clippy warning fails CI, and the `pedantic` group is on for the whole workspace (`[workspace.lints.clippy]` in the root `Cargo.toml`; each crate has `[lints] workspace = true`, which a new crate must add too). Fix a lint rather than silencing it, or use a local `#[expect(clippy::…, reason = "…")]`. Package metadata (edition, authors, repository, `license-file`, `publish = false`) lives in `[workspace.package]` and each crate inherits it with `<field>.workspace = true`; a new crate must do the same. `publish = false` is what lets `deny.toml` skip license checks for workspace crates. The code is proprietary (all rights reserved, see `LICENSE`).

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
| `tools` | `tools` | Command-line binaries (`simulate`, `batch`; later `calibrate`, `compare`…) |

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

- Commit messages: lowercase type, a space before the colon, short summary: `feat : …`, `fix : …`, `chore : …`, `docs : …`, `test : …`.
- Never tag, push or publish without an explicit request.
- `Cargo.lock` is committed (binary project).
- `.githooks/pre-commit` rejects unformatted code; enable once per clone with `git config core.hooksPath .githooks`.
- Git LFS tracks `*.png`, `*.ogg`, `*.wav`, `*.aseprite`, `*.ttf`, `*.otf`; line endings normalized to LF.
- Git convention: `main` always working, commit directly to `main` until the Steam release, then one branch per feature, a tag per phase (`phase-0`, `phase-1`…).
