# Feuille de route

**Statut actuel :** architecture décidée, workspace squelette créé. Aucun code fonctionnel pour l'instant.
**Prochaine étape :** étape 1 (`led_core`).

## Étapes

- [ ] **1. `led_core` sur Mac** (apprendre les bases de Rust)
  - [x] Supprimer le dépôt git imbriqué `crates/core/.git`
  - [x] Dépôt git dédié à `project-led`
  - [ ] Types de couleur : RGB, HSV, température de blanc (kelvin) et pixel RGBCCT à 5 canaux
  - [ ] Conversion kelvin → mélange blanc chaud / blanc froid
  - [ ] Luminosité et correction gamma
  - [ ] Transitions douces (interpolation dans le temps)
  - [ ] Tests unitaires
- [ ] **2. Piloter la bande depuis le C6**
  - [ ] Installer la toolchain (`espup` / cible RISC-V, `espflash`)
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
