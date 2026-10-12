# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Lanista is a gladiator-management game written in Rust (solo developer, Steam target, Windows + Linux/Steam Deck). The technical roadmap is [docs/Gladiator_Manager_Feuille_de_route_Rust.md](docs/Gladiator_Manager_Feuille_de_route_Rust.md) (in French); it is the reference for stack, architecture, phases and determinism rules. Gameplay rules (fighters, combat formulas, economy, colosseums, competitions) live in the design plan of 2026-09-27, which is not yet in the repo. The project is at phase 0: all crates are still `cargo new` stubs.

## Source of truth

Read the roadmap before implementing a feature. When behavior, architecture or data diverge from it, update it in the same change, in French.

## Commands

```sh
cargo build                          # whole workspace
cargo run -p lanista                 # the game executable
cargo test                           # all crates
cargo test -p lanista_core <name>    # single test (substring filter)
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo machete                        # unused dependencies
cargo deny check                     # advisories, licenses, sources (deny.toml)
```

CI ([.github/workflows/ci.yml](.github/workflows/ci.yml)) runs all of the above; clippy and tests run on both Windows and Linux so determinism breaks show up. Any clippy warning fails CI, and the `pedantic` group is on for the whole workspace (`[workspace.lints.clippy]` in the root `Cargo.toml`; each crate has `[lints] workspace = true`, which a new crate must add too). Fix a lint rather than silencing it, or use a local `#[expect(clippy::…, reason = "…")]`. Workspace crates are `publish = false`, which `deny.toml` relies on to skip license checks for them.

`[profile.dev.package."*"] opt-level = 2` in the root `Cargo.toml` is deliberate: simulations are too slow with unoptimized dependencies in debug.

## Crate naming vs. the roadmap

The roadmap uses placeholder names; the actual crates are:

| Roadmap name | Crate | Role |
| --- | --- | --- |
| `gladiator_core` | `lanista_core` | Pure game logic (combat, AI, fighters, world, economy, save) |
| `moteur` | `reesch` | Generic engine (macroquad + egui: loop, sprites, input, audio, screens) |
| `gladiator` | `lanista` | Game binary; wires core + engine |

Planned later: an ONNX inference crate, a PyO3 module for RL training (`python/` venv), and a CLI tools crate (`simuler`, `masse`, `calibrer`…). The workspace picks up any `crates/*` automatically.

## Architecture rules (enforced by crate dependencies)

`cargo deny check` enforces the forbidden dependencies below through `[bans] deny` + `wrappers` in [deny.toml](deny.toml). A new crate that may legitimately depend on a banned crate must be added to its `wrappers`; planned crate names there are `lanista_ia_onnx`, `lanista_py` and `outils`.

- `lanista_core` must never depend on macroquad, egui, ort or pyo3 — logic is runnable headless (tools, tests, Python training).
- `reesch` must never depend on `lanista_core`; it stays game-agnostic. Only add engine features a current game phase needs.
- Refer to entities by IDs (`FighterId(u32)`) stored in `Vec`/tables, not references.
- A week is a function (`semaine_suivante(&mut GameState, &Decisions)`), a combat step is a function (`tick(&mut Arene, &mut [Box<dyn CombatAi>])`). UI only builds decisions and reads state.
- Logic runs at a fixed tick rate; rendering interpolates. Speed-ups just run more ticks per frame.
- Design data (classes, spells, arenas) lives in RON files under `data/`, not in code. Saves are versioned JSON with migrations.

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
- Git LFS tracks `*.png`, `*.ogg`, `*.wav`, `*.aseprite`; line endings normalized to LF.
- Git convention: `main` always working, one branch per feature, a tag per phase (`phase-0`, `phase-1`…).
