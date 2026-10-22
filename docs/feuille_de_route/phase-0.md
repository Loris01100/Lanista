# Phase 0 — Préparation

Objectif : un projet qui compile, et la preuve que les briques risquées fonctionnent ensemble avant d'écrire du gameplay.

- Installer `rustup`, VS Code, rust-analyzer, CodeLLDB, Git et Git LFS, Python 3 avec `venv`. Sous Linux, aussi les en-têtes ALSA (`libasound2-dev`), nécessaires à l'audio de macroquad ; la CI les installe.
- Si Rust est nouveau : lire les chapitres 1 à 10 du *Rust Book* (possession, emprunts, structs, enums, traits, gestion d'erreurs), puis les chapitres 13 et 15 au besoin.
- Créer le dépôt GitHub, le workspace et les crates vides.
- Configurer `.gitignore` (`target/`, `python/.venv/`, `*.onnx` de travail), `.gitattributes` (`* text=auto eol=lf` et les règles LFS pour `*.png`, `*.ogg`, `*.wav`, `*.aseprite`).
- Profil de développement rapide pour les dépendances, sinon les simulations seront lentes en debug :

```toml
[profile.dev.package."*"]
opt-level = 2
```

- Profil de publication : `lto = "thin"` et `codegen-units = 1` dans `[profile.release]`, pour `batch` et le build Steam. Version minimale de Rust déclarée dans `[workspace.package]` (`rust-version = "1.91"`, la plus haute exigée par les dépendances, `tract`), héritée par chaque crate avec `rust-version.workspace = true`.

- Prototype 1 : une fenêtre macroquad qui affiche un sprite Kenney et un bouton egui. Vérifier tout de suite la compatibilité des versions entre macroquad et l'intégration egui. Fait : macroquad 0.4.16 avec egui-macroquad 0.17.3 (egui 0.31.1), figés par `Cargo.lock`. egui-macroquad suit egui avec retard, donc ne pas mettre egui à jour séparément.
- Prototype 2 : un module PyO3 minimal (`gladiator_py.addition(1, 2)`) compilé avec `maturin develop`, importé depuis Python. Fait : `lanista_py.addition`, compilé avec `maturin develop`.
- Prototype 3 : charger un petit modèle ONNX factice avec `ort`, puis avec `tract`, sous Windows et Linux. Fait : les deux fonctionnent ; `tract` est retenu (100 % Rust, lié statiquement, rien à livrer avec le jeu) et `ort` est retiré. Aucun des deux ne garantit des flottants identiques d'un processeur à l'autre (noyaux SIMD choisis à l'exécution) : en phase 7, enregistrer les décisions de l'IA dans les replays, ou écrire l'inférence du MLP à la main.
- CI GitHub Actions : `cargo fmt --check`, `cargo clippy`, `cargo test` sous Windows et Linux.
- Convention Git : `main` toujours fonctionnelle, commits directs sur `main` jusqu’à la publication, puis une branche par fonctionnalité, un tag par phase (`phase-0`, `phase-1`…).

**Jalon :** `cargo run -p gladiator` ouvre une fenêtre avec un sprite et un bouton ; `python -c "import gladiator_py"` fonctionne ; la CI est verte. Atteint : tag `phase-0`.
