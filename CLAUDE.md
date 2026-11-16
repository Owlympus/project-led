# project-led

Lampe « barre » façon Philips Hue (bande LED dans un tube acrylique), pilotée par un **ESP32-C6**, firmware en **Rust**.
Objectif final : chaque lampe apparaît dans l'app **Maison** de l'iPhone via **Matter**, appairée à l'Apple TV 4K.

## À lire en début de session

- [.claude/ROADMAP.md](.claude/ROADMAP.md) : où on en est, prochaine étape, questions ouvertes
- [.claude/decisions/](.claude/decisions/README.md) : les ADR (décisions d'architecture et leur pourquoi)
- [.claude/HARDWARE.md](.claude/HARDWARE.md) : matériel, câblage, pièges
- [.claude/CHANGELOG.md](.claude/CHANGELOG.md) : historique des changements

## Façon de travailler

- **Langue : français** (code, identifiants et commentaires de code en anglais).
- **Le propriétaire du projet débute en Rust** et utilise ce projet pour apprendre : expliquer les concepts au fur et à mesure (ownership, traits, `no_std`, async…), avancer par petites étapes, privilégier un code lisible à un code astucieux.
- Toute décision structurante → nouvel ADR dans `.claude/decisions/` (voir le modèle) et mise à jour de l'index.
- Toute étape terminée → entrée dans `.claude/CHANGELOG.md` et mise à jour de `.claude/ROADMAP.md`.

## Structure

Workspace Cargo (`crates/*`) :

| Crate | Rôle | Cible |
|---|---|---|
| `led_core` (`crates/core`) | Couleurs, température de blanc, transitions, effets. Indépendant du matériel. | `no_std`, testable sur Mac |
| `ws2805` (`crates/ws2805`) | Pilote de la bande WS2805 (RGB + blanc chaud + blanc froid) | `no_std` |
| *(à venir)* `firmware` | Programme du C6 : assemble bande + Wi-Fi + Matter | ESP32-C6 (`riscv32imac`) |

`led_core` et `ws2805` doivent rester réutilisables par un futur projet master/slave (cf. ADR-0005).

## Commandes

```bash
cargo test -p led_core   # tests sur le Mac
```
