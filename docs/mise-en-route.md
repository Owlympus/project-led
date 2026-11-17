# Installation et mise en route

Ce guide va de zéro (un Mac sans Rust) jusqu'à une carte qui tourne et se connecte au Wi-Fi. Il suit l'état actuel du projet : la bande LED et Matter viendront s'y ajouter au fil des étapes (voir [ROADMAP](../.claude/ROADMAP.md)).

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
- dans les logs : `Embassy started`, `RMT at 80000000 Hz`, puis un `heartbeat #…` toutes les 2 s et `strip: red`, `strip: green`… chaque seconde ;
- sur la carte : la LED RGB qui fait lentement le tour des couleurs (≈ 7 s par tour) ;
- un avertissement `Wi-Fi disabled` : normal, voir l'étape suivante.

Pour plus de détails dans les logs : `ESP_LOG=debug cargo run` (le niveau par défaut, `info`, est réglé dans `.cargo/esp-config.toml`).

## 7. Activer le Wi-Fi 🧪

Le nom et le mot de passe du réseau sont lus **à la compilation**, dans deux variables d'environnement. Ils ne sont jamais écrits dans le dépôt (voir [ADR-0008](../.claude/decisions/0008-identifiants-wifi-a-la-compilation.md)).

```bash
WIFI_SSID="NomDuReseau" WIFI_PASSWORD="motdepasse" cargo run
```

Dans les logs, on attend :

```
Wi-Fi: connected to NomDuReseau (channel 6)
Wi-Fi: IP address 192.168.1.42/24
```

Pour ne pas les retaper à chaque fois, on peut les ajouter à `~/.zshrc` (le mot de passe est alors en clair dans ce fichier, qui reste sur le Mac) :

```bash
export WIFI_SSID="NomDuReseau"
export WIFI_PASSWORD="motdepasse"
```

Conditions : réseau **2,4 GHz** (le C6 ne capte pas le 5 GHz), en WPA2 ou en mode mixte WPA2/WPA3. Un réseau WPA3 seul ne fonctionne pas encore.

*Plus tard*, avec Matter, ces variables disparaîtront : l'iPhone transmettra les identifiants Wi-Fi à la lampe pendant l'appairage.

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
| `Wi-Fi disabled` dans les logs | Les variables `WIFI_SSID` / `WIFI_PASSWORD` n'étaient pas définies **au moment de la compilation**. |
| `Wi-Fi: connection failed` en boucle | Réseau 5 GHz seulement, WPA3 seul, ou mot de passe erroné. |
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
