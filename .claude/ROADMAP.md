# Feuille de route

**Statut actuel :** étape 1 terminée (`led_core` : couleurs, blancs, HSV, gamma, transitions ; 34 tests).
**Prochaine étape :** étape 2 (piloter la bande depuis le C6) : outillage, `firmware`, encodage WS2805 et code de la LED de la carte et de la bande sont prêts. Il faut maintenant la carte pour tester, puis la bande. La carte (Waveshare ESP32-C6-DEV-KIT-N8) est commandée. Il manque encore la bande et l'adaptateur de niveau.

## Étapes

- [x] **1. `led_core` sur Mac** (apprendre les bases de Rust)
  - [x] Supprimer le dépôt git imbriqué `crates/core/.git`
  - [x] Dépôt git dédié à `project-led`
  - [x] Types de couleur : `Rgb` et pixel à 5 canaux `Rgbcct`
  - [x] Température de blanc (`ColorTemperature`, en mireds) et mélange chaud / froid (`WhiteChannels::mix`)
  - [x] Luminosité (`scale`)
  - [x] Teinte / saturation (`Hsv` → `Rgb` via `From`), comme le cluster Matter Color Control
  - [x] Correction gamma 2.2 (`gamma8`, `gamma_corrected`), appliquée en dernier avant la bande
  - [x] Transitions douces (`Transition`, avec `Easing` linéaire ou ease-in-out)
  - [x] Tests unitaires (`cargo test -p led_core`)
- [ ] **2. Piloter la bande depuis le C6**
  - [x] Ajouter la cible `riscv32imac-unknown-none-elf` (`rustup`), installer `espflash` et `esp-generate` (pas besoin d'`espup` : il ne sert qu'aux puces Xtensa, pas au C6 qui est RISC-V)
  - [x] Créer le crate `firmware` (Embassy, logs par USB, `led_core` qui tourne sur la puce), hors workspace (ADR-0007)
  - [ ] Premier flash sur la carte : `espflash board-info` (8 Mo ?), puis `cargo run`
  - [x] Code pour la LED RGB de la carte (WS2812 sur GPIO8) via le RMT : `firmware/src/board_led.rs`, compilé mais pas encore testé sur la carte
  - [ ] Voir la LED faire le tour de la roue chromatique sur la vraie carte
  - [x] Encodage WS2805 testé sur Mac : pixel → octets → bits → impulsions (`ws2805::encode`, `ws2805::timing`)
  - [x] Code de la bande via le RMT : `firmware/src/strip.rs` (canal 1, GPIO2, 30 pixels, tampon statique de ~4,8 Ko), avec un test des 5 canaux
  - [ ] Sur la vraie bande : lancer le test des canaux, vérifier l'ordre (RGB, W1/W2) et le nombre de pixels
- [ ] **3. Wi-Fi**
  - [ ] Connexion au réseau 2,4 GHz
  - [ ] (optionnel) Petite page web de test / débogage
- [ ] **4. Matter** (lampe visible dans l'app Maison)
  - [ ] Intégrer `rs-matter-embassy`, appairage par QR code via BLE
  - [ ] Clusters On/Off, Level Control, Color Control (teinte/saturation, température)
  - [ ] Mémorisation du dernier état en flash
  - [ ] Bouton tactile local
- [ ] **5. Boîtier** : tube acrylique, diffusion, dissipation thermique

## Questions ouvertes

- Annonce AliExpress : vraie puce WS2805 ? Consommation en W/m ? (Résolu : 1 puce pour 3 LEDs, soit 30 pixels.)
- Sur la bande reçue : W1 = blanc chaud ou blanc froid ? (`WhiteOrder`)
- Génération de l'Apple TV 4K (Thread possible ou non) ?
- GPIO utilisée pour les données de la bande ? (candidates dans `HARDWARE.md`)
- Boîtier pas encore défini (tube acrylique, pièces imprimées en 3D, lest dans le pied…). Seule contrainte connue : pas de métal autour de l'antenne.
- Lampe finale : la C6-Zero (4 Mo) suffit-elle pour le firmware Matter ?

## Idées pour plus tard

- Projet séparé **master/slave** : un master sert de pont (et de *pont Matter*), les slaves le suivent en ESP-NOW. Réutilise `led_core` et `ws2805`.
- Matter sur Thread.
- Effets dynamiques et dégradés le long de la barre (façon Hue Gradient).
- **Hors de ce projet** (qui s'arrête à la v1 avec la bande WS2805) : une bande RGBCW maison, avec **une puce par LED** (puces WS2805 seules et LED 5050 « 5 en 1 » sur un circuit imprimé à dessiner). Aucune bande du commerce ne le propose. Point à vérifier : la tension (en 12 V, une puce pour une seule LED chauffe). `led_core` et `ws2805` resserviraient tels quels.
