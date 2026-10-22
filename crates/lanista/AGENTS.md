# lanista

The game binary: the club screen, the combat screen, saves.

- Every player text is a Fluent key of `assets/lang/<language>.ftl`, read with `lang::tr!("key", name = value)`; a grammar choice is passed as an English selector (`$role`). Tests check that each `tr!` key and each key of `fr.ftl` exist in every language's file, and that the theme's font has a glyph for each character; tests comparing texts stay in French, the default.
- egui only through `reesch::egui`; the theme is `theme.rs`; the UI is designed for an 800 px high window and scaled.
- `club_screen.rs` owns the `GameState` and only builds `Decisions` and reads the state; its views live in `club_screen/` (`colosseums.rs`, `market.rs`, `staff.rs`), player texts in `texts.rs`.
- Tests check that ten weeks on screen match the `career` tool, and that a watched fight matches `simulate` for the same seed: a change to how a week or a fight is played shows up there.
- Saves go to the platform data folder (`saves.rs`): `autosave.json` after every week, `save.json` on demand.
