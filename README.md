# Lanista

A gladiator-management game: recruit fighters, train and heal them, send them into the arena, and climb the colosseum rankings. Combats are real-time simulations that you can watch, and the same seed always replays the same fight.

Written in Rust with macroquad and egui, for Windows and Linux (Steam Deck). Planned for a free release on Steam.

> Early development (phase 6 of the [roadmap](docs/Gladiator_Manager_Feuille_de_route_Rust.md) done): the game is playable with the scripted AI and placeholder graphics. You run a club week after week against rival clubs, in duels and a season-opening melee of up to 20, and watch your fights together. The learned AI comes next.

## Build

Requires stable Rust (`rustup`) and Git LFS (`git lfs install`) for the assets. On Linux, also the ALSA headers (`libasound2-dev` on Debian/Ubuntu).

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
| `lanista_py` | Python module (PyO3) for training the combat AI |
| `lanista_ia_onnx` | Runs the trained AI model in the game (`tract`) |

## License

All rights reserved; see [LICENSE](LICENSE).
