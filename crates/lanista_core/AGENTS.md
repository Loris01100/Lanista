# lanista_core

Pure game logic. Determinism rules are in the root `AGENTS.md`.

- Combat features are opt-in on the `Arena`: `Arena::new` is bare (always hits, no stun, cut, bleed, layout, pacts nor eliminations), and `with_accuracy`, `with_rules`, `with_layout`, `with_pacts`, `with_elimination` turn them on. Career fights (`CareerFighter::duel_arena`, `melee::melee_arena`) use them all; many tests and the standard duel rely on the bare arena.
- A feature that is off must draw nothing from the arena's generator: any new draw shifts every later one, changing the `insta` snapshots of reference fights. When a snapshot changes on purpose, review it with `cargo insta review`.
- Rival fights go through the statistical model (`stat_model`, `data/stat_model.ron`), the club's through real duels. After any balancing change (classes, spells, combat rules), rerun `calibrate` then `compare` (see the root commands).
- `state.rs` tests play in a world of three clubs, all in Europe with Capoue and Rome only (`data()` with `count` 3, `game`, `game_of`); the phase 5 milestone plays in the real world.
- Entries are never removed, so ids stay valid: a fighter who leaves keeps its entry with its status (`Sold`, `Released`, moved), a closed club keeps its `Rival` entry (`Rival::is_open`), a free agent its place in `GameState::free_agents`.
- The learned AI: the models in `assets/ai/` are placeholders with no `.json` network on purpose, so every class stays on the utility AI until phase 9 trains the real ones (then rerun `calibrate` and review the snapshots).
- Values belong in `data/*.ron` (read as `DesignData`), not in code; a class, colosseum or culture has a `key` whose player text is `class-<key>`, `colosseum-<key>`, `culture-<key>` in the game's language files.
