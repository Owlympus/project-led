# ADR-0007 : Le crate `firmware` est un projet Cargo à part

- **Statut :** Accepté
- **Date :** 2026-11-17

## Contexte
`led_core` (et plus tard `ws2805`) se compilent et se testent **sur le Mac** (`cargo test`). Le `firmware`, lui, ne se compile que **pour l'ESP32-C6** (`riscv32imac-unknown-none-elf`). Il a aussi ses propres réglages : la cible par défaut et le lanceur `espflash` (dans `.cargo/config.toml`), un script d'édition de liens (`build.rs`) et des profils optimisés pour la taille.

Dans un workspace Cargo, la cible par défaut et les profils sont **communs à tous les crates**. Si on fixait la cible du C6 pour tout le workspace, `cargo test -p led_core` essaierait de lancer les tests sur le C6 et échouerait.

## Décision
`crates/firmware` est **exclu du workspace** (`exclude` dans le `Cargo.toml` racine). C'est un projet Cargo autonome, avec son propre `Cargo.lock`, son dossier `target/`, son `rust-toolchain.toml` et son `.cargo/config.toml`. Il utilise `led_core` par un chemin relatif (`path = "../core"`).

On travaille dessus depuis son dossier : `cd crates/firmware && cargo build` (ou `cargo run` pour flasher la carte et afficher ses logs).

Le squelette a été généré par `esp-generate` 2.0.0 avec les options `esp32c6`, `esp32c6-wroom-1`, `unstable-hal`, `embassy`, `log`, `esp-backtrace` et `wokwi`.

## Alternatives envisagées
- **Firmware membre du workspace**, en passant la cible à chaque commande (`--target`) : facile à oublier et source d'erreurs. Et la cible par package (`forced-target`) n'existe que sur Rust nightly.
- **Firmware dans un dépôt séparé** : plus lourd à gérer pour un seul projet, alors que `led_core` évolue en même temps.

## Conséquences
- `cargo test`, `cargo clippy` et `cargo fmt` à la racine ne voient **pas** le firmware : il faut aussi les lancer dans `crates/firmware`.
- Le firmware n'a pas de tests unitaires (`test = false`) : la logique testable va dans `led_core`, qui reste testable sur le Mac.
- Deux `Cargo.lock` à commiter (racine et firmware).
