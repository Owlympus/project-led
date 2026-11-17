# Installation et mise en route

Ce guide va de zéro (un Mac sans Rust) jusqu'à une lampe appairée dans l'app Maison de l'iPhone. Il suit l'état actuel du projet et s'enrichira au fil des étapes (voir [ROADMAP](../.claude/ROADMAP.md)).

> **Statut :** le firmware compile, mais n'a pas encore été testé sur une vraie carte. Les sections marquées 🧪 décrivent ce qu'on *attend* ; elles seront corrigées après les premiers essais.

## 1. Ce qu'il faut

**Sur le Mac**
- macOS avec les outils en ligne de commande d'Apple (`git`) : `xcode-select --install` s'ils manquent.
- Une connexion internet (pour télécharger Rust et les dépendances).

**Matériel**
- Une carte **Waveshare ESP32-C6-DEV-KIT-N8** (voir [HARDWARE](../.claude/HARDWARE.md)).
- Un câble **USB-C data** (un câble « charge seule » ne laisse passer que le courant : la carte s'allume, mais le Mac ne la voit pas).
- Pour la bande : la liste complète et le câblage sont dans [HARDWARE](../.claude/HARDWARE.md). Pas nécessaire pour les premiers essais.

## 2. Installer Rust et les outils

À faire une seule fois.

1. **Rust** (via `rustup`, le gestionnaire de versions de Rust, un peu comme `phpenv`) :

   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

   Puis ouvrir un nouveau terminal (pour que `cargo` soit dans le `PATH`) et vérifier :

   ```bash
   rustc --version
   ```

2. **La cible du C6.** Le Mac compile pour lui-même (`aarch64-apple-darwin`) ; pour le C6, une puce RISC-V sans système d'exploitation, il faut ajouter sa cible :

   ```bash
   rustup target add riscv32imac-unknown-none-elf
   ```

3. **`espflash`**, qui envoie le programme sur la carte et affiche ses logs :

   ```bash
   cargo install espflash --locked
   ```

4. *(Facultatif)* **`esp-generate`**, le générateur de projets ESP en Rust. Il a servi à créer `crates/firmware` ; inutile pour travailler sur le projet.

   ```bash
   cargo install esp-generate --locked
   ```

Pas besoin d'`espup` : il ne sert qu'aux anciennes puces ESP32 (architecture Xtensa), pas au C6.

## 3. Récupérer le projet et lancer les tests

```bash
git clone https://github.com/Owlympus/project-led.git
cd project-led
cargo test
```

`cargo test` compile et teste **sur le Mac** les crates indépendants du matériel (`led_core`, `ws2805`). Tout doit passer, sans carte branchée.

Le dépôt contient deux projets Cargo (voir [ADR-0007](../.claude/decisions/0007-firmware-hors-workspace.md)) :

| Dossier | Ce que c'est | Où lancer les commandes |
|---|---|---|
| racine | `led_core` et `ws2805`, testés sur le Mac | `project-led/` |
| `crates/firmware` | le programme du C6 | `project-led/crates/firmware/` |

## 4. Compiler le firmware

```bash
cd crates/firmware
cargo build
```

La première compilation est longue (plusieurs minutes) : Cargo télécharge les dépendances et recompile la bibliothèque de base de Rust pour le C6. Les suivantes ne prennent que quelques secondes.

## 5. Brancher la carte 🧪

1. Brancher la carte au Mac en USB-C.
2. Vérifier que le Mac la voit et qu'elle a bien **8 Mo** de flash :

   ```bash
   espflash board-info
   ```

   S'il y a plusieurs ports série, `espflash` demande lequel choisir : prendre celui qui contient `usbmodem`.

## 6. Flasher et voir les logs 🧪

Depuis `crates/firmware` :

```bash
cargo run
```

`cargo run` compile, écrit le programme dans la flash de la carte, puis affiche ses logs. `Ctrl+C` pour quitter (le programme continue de tourner sur la carte).

Ce qu'on doit voir :
- dans les logs : `Embassy started`, `RMT at 80000000 Hz`, puis `strip test: red`, `strip test: green`… (une fois, au démarrage) et un `heartbeat #…` toutes les 2 s ;
- un **QR code** dessiné dans les logs, pour l'appairage (étape suivante) ;
- sur la carte : la LED RGB qui fait lentement le tour des couleurs (≈ 7 s par tour).

`cargo run --release` produit un programme optimisé, plus petit et plus rapide (la compilation est plus longue).

Pour plus de détails dans les logs : `ESP_LOG=debug cargo run` (le niveau par défaut, `info`, est réglé dans `.cargo/esp-config.toml`).

## 7. Appairer la lampe avec l'app Maison 🧪

La lampe est un appareil **Matter** ([ADR-0009](../.claude/decisions/0009-matter-avec-rs-matter-embassy.md)). Il faut un concentrateur domestique (ici l'**Apple TV**) et l'iPhone sur le même réseau Wi-Fi **2,4 GHz** (le C6 ne capte pas le 5 GHz ; un réseau qui mélange les deux bandes convient).

1. Lancer `cargo run` et garder les logs ouverts : la lampe attend l'appairage par **Bluetooth**.
2. Sur l'iPhone, app **Maison** → **+** → **Ajouter un accessoire**.
3. Scanner le QR code affiché dans les logs. Si le scan ne marche pas, choisir « Plus d'options » et saisir le code de test **3497-011-2332**.
4. L'app prévient que l'accessoire n'est **pas certifié** (normal : on utilise les certificats de test de Matter) → **Ajouter quand même**.
5. L'iPhone transmet le nom et le mot de passe du Wi-Fi à la lampe, qui s'y connecte toute seule. On voit le déroulé dans les logs.
6. Une fois ajoutée, la lampe « Lampe barre » s'allume et s'éteint depuis l'app : la bande passe en blanc chaud avec un fondu, et les logs affichent `Matter: light on` / `Matter: light off`.

**Limite actuelle :** rien n'est encore enregistré en flash. Après un redémarrage (ou un nouveau `cargo run`), il faut **supprimer la lampe** de l'app Maison puis refaire l'appairage.

## 8. Brancher la bande LED 🧪

Avant de brancher quoi que ce soit en 12 V, relire [HARDWARE](../.claude/HARDWARE.md), sections *Câblage* et *Pièges connus*. En résumé :
- la carte reste alimentée par l'USB, la bande par le 12 V : **seule la masse (GND) est commune** ;
- les données partent de **GPIO2**, passent par l'adaptateur de niveau 74AHCT125 puis une résistance de 330 Ω, et arrivent sur l'entrée **DIN** de la bande ;
- un condensateur de 1000 µF entre +12 V et GND de la bande.

Au démarrage, le firmware lance un **test des canaux** : toute la bande s'allume en rouge, vert, bleu, blanc chaud, blanc froid puis s'éteint, 1 s chacun, et les logs annoncent la couleur attendue. Si la bande montre autre chose, la marche à suivre est dans [HARDWARE](../.claude/HARDWARE.md), section *Protocole de la bande*.

## 9. En cas de problème

| Symptôme | Piste |
|---|---|
| `espflash` ne trouve aucun port | Câble charge seule ? Essayer un autre câble, un autre port USB du Mac. |
| Le flash échoue ou reste bloqué | Passer la carte en mode téléchargement : maintenir **BOOT**, appuyer sur **RESET**, relâcher **BOOT**, puis relancer `cargo run`. |
| `error[E0463]: can't find crate for core` | La cible n'est pas installée : `rustup target add riscv32imac-unknown-none-elf`. |
| L'app Maison ne trouve pas la lampe | Bluetooth de l'iPhone activé ? Lampe déjà appairée avant un redémarrage : la supprimer de l'app puis recommencer. |
| L'appairage échoue à l'étape du Wi-Fi | Réseau 5 GHz seulement, ou WPA3 seul. |
| Panique au démarrage de Matter (*bump allocator*) | Augmenter `BUMP_SIZE` dans `firmware/src/matter.rs`. |
| La bande clignote au hasard | Adaptateur de niveau absent ou masse non commune (voir HARDWARE, *Pièges connus*). |

## 10. Avant de commiter

Les règles du projet (branches, PR, messages de commit) sont dans [CLAUDE.md](../CLAUDE.md). Les vérifications :

```bash
cargo fmt && cargo clippy --all-targets && cargo test
```

à la racine, puis, si le firmware a changé :

```bash
cd crates/firmware && cargo fmt && cargo clippy --all-targets
```
