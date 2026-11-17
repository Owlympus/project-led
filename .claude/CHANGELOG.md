# Changelog

Format inspiré de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/). Les dates sont au format AAAA-MM-JJ.

## [Non publié]

### Ajouté
- 2026-11-16 : `led_core` : types `Rgb` et `Rgbcct` avec réglage de luminosité, `ColorTemperature` (mireds/kelvin) et `WhiteChannels::mix` pour le mélange blanc chaud / froid ; 12 tests unitaires. Voir ADR-0006.
- 2026-11-16 : workspace Cargo avec les crates squelettes `led_core` (`crates/core`) et `ws2805`.
- 2026-11-16 : documentation du projet : `CLAUDE.md`, `.claude/ROADMAP.md`, `.claude/HARDWARE.md`, `.claude/CHANGELOG.md` et ADR 0001 à 0005 dans `.claude/decisions/`.

- 2026-11-16 : dépôt git dédié à `project-led` (auparavant le dossier parent `Projects/Rust`), avec un `.gitignore` (`target/`, `.idea/`, `.DS_Store`).

### Décidé
- 2026-11-16 : ESP32-C6, bande WS2805 12 V, Rust esp-hal + Embassy, une lampe = un appareil Matter appairé à l'Apple TV, découpage en crates réutilisables (voir les ADR).
