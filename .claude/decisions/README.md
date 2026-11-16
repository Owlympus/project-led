# Décisions d'architecture (ADR)

Un ADR (*Architecture Decision Record*) note **une** décision : le contexte, ce qu'on a choisi, pourquoi, et ce que ça implique.
On ne réécrit pas un ADR accepté : si on change d'avis, on en crée un nouveau qui le **remplace** (statut « Remplacé par ADR-XXXX »).

| N° | Titre | Statut | Date |
|---|---|---|---|
| [0001](0001-microcontroleur-esp32-c6.md) | Microcontrôleur ESP32-C6 | Accepté | 2026-11-16 |
| [0002](0002-bande-ws2805-12v.md) | Bande LED WS2805 RGBCCT 12 V | Accepté | 2026-11-16 |
| [0003](0003-rust-esp-hal-embassy.md) | Rust `no_std` avec esp-hal + Embassy | Accepté | 2026-11-16 |
| [0004](0004-matter-par-lampe-via-apple-tv.md) | Chaque lampe est un appareil Matter appairé à l'Apple TV | Accepté | 2026-11-16 |
| [0005](0005-decoupage-en-crates.md) | Découpage du workspace en crates réutilisables | Accepté | 2026-11-16 |

Modèle : [TEMPLATE.md](TEMPLATE.md)
