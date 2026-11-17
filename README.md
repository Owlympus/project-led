# project-led

Une lampe « barre » façon Philips Hue : une bande LED RGB + blanc chaud + blanc froid (WS2805) dans un tube, pilotée par un **ESP32-C6** avec un firmware en **Rust**. Objectif : chaque lampe apparaît dans l'app **Maison** de l'iPhone grâce à **Matter**.

Projet personnel, qui sert aussi à apprendre Rust.

- **Démarrer :** [docs/mise-en-route.md](docs/mise-en-route.md) (installation, flash de la carte, Wi-Fi)
- **Où en est le projet :** [.claude/ROADMAP.md](.claude/ROADMAP.md)
- **Matériel et câblage :** [.claude/HARDWARE.md](.claude/HARDWARE.md)
- **Décisions d'architecture :** [.claude/decisions/](.claude/decisions/README.md)
- **Historique :** [.claude/CHANGELOG.md](.claude/CHANGELOG.md)

| Crate | Rôle |
|---|---|
| `led_core` (`crates/core`) | Couleurs, températures de blanc, transitions. Indépendant du matériel, testé sur le Mac. |
| `ws2805` (`crates/ws2805`) | Encodage des pixels pour la bande WS2805. Testé sur le Mac. |
| `firmware` (`crates/firmware`) | Le programme de l'ESP32-C6 (Embassy) : bande, Wi-Fi, puis Matter. |
