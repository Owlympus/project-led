# Feuille de route

**Statut actuel :** étape 1 terminée (`led_core` : couleurs, blancs, HSV, gamma, transitions ; 34 tests).
**Prochaine étape :** étape 2 (piloter la bande depuis le C6). Il faut la carte ESP32-C6, la bande et l'adaptateur de niveau.

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
  - [ ] Ajouter la cible `riscv32imac-unknown-none-elf` (`rustup`) et installer `espflash` (pas besoin d'`espup` : il ne sert qu'aux puces Xtensa, pas au C6 qui est RISC-V)
  - [ ] Créer le crate `firmware`, faire clignoter une LED
  - [ ] Pilote WS2805 via le périphérique RMT (`ws2805`)
  - [ ] Afficher une couleur et un blanc sur la bande
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

- Annonce AliExpress : vraie puce WS2805 ? Combien de LEDs par puce ? Consommation en W/m ?
- Génération de l'Apple TV 4K (Thread possible ou non) ?
- Modèle exact de la carte ESP32-C6 et taille de sa flash ?
- GPIO utilisée pour les données de la bande ?

## Idées pour plus tard

- Projet séparé **master/slave** : un master sert de pont (et de *pont Matter*), les slaves le suivent en ESP-NOW. Réutilise `led_core` et `ws2805`.
- Matter sur Thread.
- Effets dynamiques et dégradés le long de la barre (façon Hue Gradient).
