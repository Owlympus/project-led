# ADR-0005 : Découpage du workspace en crates réutilisables

- **Statut :** Accepté
- **Date :** 2026-11-16

## Contexte
La logique des couleurs et le pilote de bande ne dépendent pas de la façon dont la lampe est contrôlée. Un futur projet master/slave (cf. ADR-0004) devra pouvoir les réutiliser.

## Décision
Workspace Cargo avec :
- **`led_core`** : couleurs, température de blanc, transitions, effets. `no_std`, sans dépendance matérielle, **testable sur Mac**.
- **`ws2805`** : pilote de la bande. `no_std`.
- **`firmware`** (à venir) : seul crate lié à l'ESP32-C6, Wi-Fi et Matter.

## Conséquences
- Les tests de `led_core` tournent avec un simple `cargo test`, sans carte branchée : idéal pour apprendre Rust.
- Le firmware ciblera `riscv32imac-unknown-none-elf`, tandis que les libs restent compilables pour le Mac : la configuration de cible devra être faite au niveau du crate `firmware`, pas du workspace.
