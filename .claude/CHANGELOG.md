# Changelog

Format inspiré de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/). Les dates sont au format AAAA-MM-JJ.

## [Non publié]

### Retiré
- 2026-11-17 : `firmware` : réglage `[unstable] build-std` (sans effet sur Rust stable, la bibliothèque de base vient de `rustup target add`) ; phrase fausse à ce sujet corrigée dans `docs/mise-en-route.md`.
- 2026-11-17 : nettoyage : fichiers du simulateur Wokwi (`wokwi.toml`, `diagram.json`, inutiles sans simulation Bluetooth ni de WS2805) ; dépendances inutilisées du firmware (`critical-section`, `portable-atomic`, `tinyrlibc`) ; mentions périmées dans `CLAUDE.md`, `HARDWARE.md`, le ROADMAP et un commentaire de `matter.rs`.

### Modifié
- 2026-11-17 : bande à **30 pixels** (1 puce WS2805 pour 3 LEDs) au lieu de 90 : `STRIP_PIXELS`, tampon RMT ramené à ~4,8 Ko ; doc (HARDWARE, ADR-0002, ROADMAP).

### Ajouté
- 2026-11-17 : `led_core` : état de la lampe (`LightState` : allumage, luminosité, `LightColor` en teinte/saturation ou température) et son pixel (`LightState::pixel`) ; conversions des échelles Matter 0..=254 (`light::matter_scale`) ; 7 tests. Début de l'étape 4b.
- 2026-11-17 : `firmware` : **Matter** (`matter.rs`, `rs-matter-embassy` en dépendance Git figée) : appairage Bluetooth avec QR code, Wi-Fi fourni par l'iPhone, lampe On/Off ; la bande suit les demandes avec un fondu de 400 ms (`led_core::Transition`) après le test des canaux ; tas porté à 100 Ko ; LED de la carte dans sa propre tâche. `wifi.rs` retiré (ADR-0008 remplacé par ADR-0009).
- 2026-11-17 : doc : `README.md` et guide d'installation et de mise en route (`docs/mise-en-route.md`) ; boîte à idées des signaux lumineux d'état (vague rouge / bleue, pixel d'alerte Wi-Fi) dans le ROADMAP.
- 2026-11-17 : `firmware` : Wi-Fi (`wifi.rs`) : connexion au réseau avec reconnexion automatique, adresse IP par DHCP affichée dans les logs (`esp-radio` 1.0 bêta, `embassy-net` 0.9) ; tas de 64 Ko (`esp-alloc`) exigé par le pilote radio ; identifiants `WIFI_SSID` / `WIFI_PASSWORD` lus à la compilation (ADR-0008).
- 2026-11-17 : `firmware` : pilote de la bande (`strip.rs`, canal RMT 1 sur GPIO2, tampon statique `ConstStaticCell` de 90 pixels) et tâche de test des 5 canaux ; conversion impulsions → codes RMT partagée (`pulse_code.rs`). `ws2805` : `strip_bytes` accepte tout itérateur de pixels ; 1 test.
- 2026-11-17 : `firmware` : la LED RGB de la carte (WS2812, GPIO8) fait le tour de la roue chromatique via le RMT à 80 MHz (`board_led.rs`, `led_core` → `ws2805` → RMT) ; `main.rs` déplacé dans `src/`. `ws2805` : `Pulse::to_ticks` (ns → ticks d'horloge, arrondi) ; 2 tests.
- 2026-11-17 : `ws2805` : encodage pixel → octets (R, G, B, W1, W2 ; `WhiteOrder` pour l'ordre des blancs) → bits (poids fort en premier) → impulsions, sous forme d'itérateurs ; timings WS2805 (fiche technique) et WS2812 (LED de la carte) ; 10 tests + 1 exemple de doc.
- 2026-11-17 : crate `firmware` (généré par `esp-generate` 2.0.0, esp-hal 1.2 + Embassy) : deux tâches en parallèle (battement de cœur et tour de la roue chromatique calculé par `led_core`), logs par USB ; hors workspace (ADR-0007).
- 2026-11-17 : doc : carte de développement Waveshare ESP32-C6-DEV-KIT-N8 dans `HARDWARE.md` (brochage, broches à éviter, alimentation, antenne) ; alternatives Pico 2 W, ESP32 et S3 dans l'ADR-0001.
- 2026-11-17 : `led_core` : transitions (`Transition::new` / `instant` / `at` / `is_done`) avec `Easing` (`Linear`, `EaseInOut` par défaut) ; le temps écoulé est fourni par l'appelant, sans horloge dans le crate ; 10 tests. **Fin de l'étape 1.**
- 2026-11-17 : `led_core` : correction gamma 2.2 par table (`gamma8`), méthodes `Rgb::gamma_corrected` et `Rgbcct::gamma_corrected` ; une valeur non nulle reste ≥ 1 pour que la lampe ne s'éteigne pas au minimum ; 6 tests.
- 2026-11-17 : `led_core` : type `Hsv` (teinte sur 16 bits, saturation, luminosité) et conversion `From<Hsv> for Rgb` en calcul entier ; 6 tests.
- 2026-11-16 : `led_core` : types `Rgb` et `Rgbcct` avec réglage de luminosité, `ColorTemperature` (mireds/kelvin) et `WhiteChannels::mix` pour le mélange blanc chaud / froid ; 12 tests unitaires. Voir ADR-0006.
- 2026-11-16 : workspace Cargo avec les crates squelettes `led_core` (`crates/core`) et `ws2805`.
- 2026-11-16 : documentation du projet : `CLAUDE.md`, `.claude/ROADMAP.md`, `.claude/HARDWARE.md`, `.claude/CHANGELOG.md` et ADR 0001 à 0005 dans `.claude/decisions/`.
- 2026-11-16 : dépôt git dédié à `project-led` (auparavant le dossier parent `Projects/Rust`), avec un `.gitignore` (`target/`, `.idea/`, `.DS_Store`).

### Décidé
- 2026-11-17 : le projet s'arrête à la v1 avec la bande WS2805 RGBCCT (RGBW écarté, précision dans l'ADR-0002) ; une bande RGBCW maison à une puce par LED est notée comme idée hors projet dans le ROADMAP.
- 2026-11-16 : ESP32-C6, bande WS2805 12 V, Rust esp-hal + Embassy, une lampe = un appareil Matter appairé à l'Apple TV, découpage en crates réutilisables (voir les ADR).
