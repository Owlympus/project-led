# project-led

Lampe « barre » façon Philips Hue (bande LED dans un tube acrylique), pilotée par un **ESP32-C6**, firmware en **Rust**.
Objectif final : chaque lampe apparaît dans l'app **Maison** de l'iPhone via **Matter**, appairée à l'Apple TV 4K.

## À lire en début de session

- [docs/mise-en-route.md](docs/mise-en-route.md) : installation, flash, appairage (à tenir à jour quand la mise en route change)
- [.claude/ROADMAP.md](.claude/ROADMAP.md) : où on en est, prochaine étape, questions ouvertes
- [.claude/decisions/](.claude/decisions/README.md) : les ADR (décisions d'architecture et leur pourquoi)
- [.claude/HARDWARE.md](.claude/HARDWARE.md) : matériel, câblage, pièges
- [.claude/CHANGELOG.md](.claude/CHANGELOG.md) : historique des changements

## Façon de travailler

- **Langue : français** (code, identifiants et commentaires de code en anglais).
- **Le propriétaire du projet débute en Rust** et utilise ce projet pour apprendre : expliquer les concepts au fur et à mesure (ownership, traits, `no_std`, async…), avancer par petites étapes, privilégier un code lisible à un code astucieux.
- **Il vient de PHP** : quand c'est utile, comparer avec PHP (traits/interfaces, Composer ↔ Cargo, PHPUnit ↔ `cargo test`…) et expliquer les conventions de Rust qui diffèrent.
- Conventions retenues : tests unitaires dans le fichier (`#[cfg(test)] mod tests` en bas), dossier `tests/` réservé aux tests de l'API publique.
- Toute décision structurante → nouvel ADR dans `.claude/decisions/` (voir le modèle) et mise à jour de l'index.
- Toute étape terminée → entrée dans `.claude/CHANGELOG.md` et mise à jour de `.claude/ROADMAP.md`.

## Git

- `main` reste stable : on n'y travaille pas directement.
- Une branche par brique : `feat/…` (fonctionnalité), `fix/…` (correction), `docs/…` (documentation). Ex. : `feat/led-core-hsv`.
- Messages de commit en anglais, préfixés par le crate concerné (`led_core: …`), avec un corps qui liste les changements.
- Chaque branche est poussée puis fusionnée dans `main` via une **pull request** sur GitHub (`gh pr create`).
- Claude ouvre les PR, mais **ne fusionne qu'après le feu vert explicite** du propriétaire. La fusion se fait en squash, en supprimant la branche (`gh pr merge <n> --squash --delete-branch`).
- Branche principale : `main` (choix confirmé, on ne passe pas à `master`).
- Avant de commiter : `cargo fmt`, `cargo clippy --all-targets`, `cargo test` (à la racine, et `fmt` + `clippy` dans `crates/firmware` s'il a changé).

## Structure

Workspace Cargo (`crates/*`, sauf `firmware`, cf. ADR-0007) :

| Crate | Rôle | Cible |
|---|---|---|
| `led_core` (`crates/core`) | Couleurs, température de blanc, transitions, effets. Indépendant du matériel. | `no_std`, testable sur Mac |
| `ws2805` (`crates/ws2805`) | Pilote de la bande WS2805 (RGB + blanc chaud + blanc froid) : encodage testable sur Mac, envoi RMT à venir | `no_std`, testable sur Mac |
| `firmware` (`crates/firmware`, **hors workspace**) | Programme du C6 : assemble bande + Wi-Fi + Matter (Embassy) | ESP32-C6 (`riscv32imac`) |

`led_core` et `ws2805` doivent rester réutilisables par un futur projet master/slave (cf. ADR-0005).

## Commandes

```bash
cargo test                           # tous les tests sur le Mac (led_core, ws2805)
cargo test -p led_core               # les tests d'un seul crate
cargo clippy -p led_core --all-targets   # analyse du code (lint)
cargo fmt                            # formatage automatique
```

Firmware (depuis `crates/firmware`, car il est hors workspace) :

```bash
cargo build                          # compile pour le C6
cargo run                            # compile, flashe la carte par USB et affiche les logs
cargo run --release                  # idem en optimisé (plus petit, plus rapide)
cargo clippy --all-targets && cargo fmt
```
