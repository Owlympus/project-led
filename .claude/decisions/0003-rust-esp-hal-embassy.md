# ADR-0003 : Rust `no_std` avec esp-hal + Embassy

- **Statut :** Accepté
- **Date :** 2026-11-16

## Contexte
Le projet sert aussi à **apprendre Rust**. Deux grandes approches existent sur ESP32 : Rust « bare metal » (`esp-hal`) ou Rust au-dessus du framework C d'Espressif (`esp-idf-hal`, avec la `std`).

## Décision
**`esp-hal` + Embassy** (async), en `no_std`, pour le firmware. Matter via **`rs-matter` / `rs-matter-embassy`**.

## Alternatives envisagées
- **`esp-idf-hal` / `esp-idf-svc`** : accès à la `std` et aux piles réseau d'ESP-IDF, mais une compilation plus lourde (toolchain C) et moins dans la direction actuelle de l'écosystème.
- **SDK Matter officiel d'Espressif (C++)** : plus mûr, mais ce n'est pas du Rust.

## Conséquences
- Tout en Rust, outillage simple (`espflash`, `probe-rs`).
- `rs-matter` est encore jeune : API changeante, peu de doc, appareil « non certifié » dans l'app Maison (contournable).
- L'async (Embassy) est un concept à apprendre, introduit progressivement.
