# Lanista

A gladiator-management game: recruit fighters, train and heal them, send them into the arena, and climb the colosseum rankings. Combats are real-time simulations that you can watch, and the same seed always replays the same fight.

Written in Rust with macroquad and egui, for Windows and Linux (Steam Deck). Planned for a free release on Steam.

> Early development (phase 0 of the [roadmap](docs/Gladiator_Manager_Feuille_de_route_Rust.md)): nothing is playable yet.

## Build

Requires stable Rust (`rustup`) and Git LFS (`git lfs install`) for the assets.

```sh
git config core.hooksPath .githooks   # format check before each commit
cargo run -p lanista
cargo test
```

## Layout

| Crate | Role |
| --- | --- |
| `lanista_core` | Game logic, no graphics: combat, AI, fighters, world, economy, saves |
| `reesch` | Generic engine: game loop, sprites, input, audio, egui screens |
| `lanista` | The game executable |

## License

All rights reserved; see [LICENSE](LICENSE).
