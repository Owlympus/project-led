# Feuille de route

**Statut actuel :** étape 1 terminée ; le code des étapes 2, 3 et 4a compile mais n'a pas encore tourné sur la carte. Le code des étapes 4b (luminosité et couleurs), 4c (sauvegarde en flash) et 4d (bouton tactile) compile aussi.
**Prochaine étape :** les tests sur le matériel dès que la carte arrive ; en attendant, les ambiances de la boîte à idées, ou l'étape 5 (boîtier). La carte (Waveshare ESP32-C6-DEV-KIT-N8) est commandée. Il manque encore la bande et l'adaptateur de niveau.

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
- [x] **3. Wi-Fi**
  - [x] Connexion au réseau 2,4 GHz, reconnexion, DHCP (`esp-radio` + `embassy-net`). D'abord avec des identifiants lus à la compilation (ADR-0008), puis repris par Matter (ADR-0009) : `wifi.rs` a été retiré.
  - [ ] (optionnel) Petite page web de test / débogage
- [ ] **4. Matter** (lampe visible dans l'app Maison, ADR-0009)
  - [x] **4a.** `rs-matter-embassy` intégré : appairage par QR code via Bluetooth, Wi-Fi fourni par l'iPhone, lampe **On/Off** qui allume la bande en blanc 3000 K avec un fondu (`firmware/src/matter.rs`) ; compilé, pas encore testé sur la carte
  - [ ] Sur la vraie carte : appairer avec l'app Maison, allumer / éteindre depuis l'iPhone
  - [x] **4b.** Level Control et Color Control (luminosité, teinte/saturation, température), reliés à `led_core`
    - [x] `led_core::LightState` : allumage, luminosité, couleur (teinte/saturation ou température) → pixel, et conversions des échelles Matter (`matter_scale`)
    - [x] `matter.rs` : lampe *Extended Color Light* (Level Control, Color Control, Identify, Scenes) ; la bande suit un `LightState` ; compilé, pas encore testé sur la carte
    - [x] Transitions faites par `rs-matter`, lissées sur 100 ms par la bande (ADR-0010)
  - [ ] Sur la vraie carte : luminosité, couleurs et blancs depuis l'app Maison
  - [x] **4c.** Mémorisation de l'appairage et du dernier état en flash : partition NVS, `SeqMapKvBlobStore` (ADR-0011) ; compilé, pas encore testé sur la carte
  - [ ] Sur la vraie carte : la lampe reste appairée et revient à son dernier état après un redémarrage
  - [x] **4d.** Bouton tactile local (TTP223 sur GPIO3) : appui = bascule via Matter, maintien 5 s au démarrage = oubli de l'appairage (ADR-0012) ; compilé, pas encore testé
  - [ ] Sur la vraie carte : acheter et brancher le TTP223, vérifier son mode et les deux gestes
- [ ] **5. Boîtier** : tube acrylique, diffusion, dissipation thermique

## Questions ouvertes

- Bande reçue : consommation réelle en W/m (pour dimensionner l'alimentation 12 V) ?
- Sur la bande reçue : W1 = blanc chaud ou blanc froid ? (`WhiteOrder`) Températures réelles des deux blancs ? (`STRIP_WHITE_CHANNELS`, 2700 K / 6500 K supposés)
- Génération de l'Apple TV 4K (Thread possible ou non) ?
- GPIO utilisée pour les données de la bande ? (candidates dans `HARDWARE.md`)
- Boîtier pas encore défini (tube acrylique, pièces imprimées en 3D, lest dans le pied…). Seule contrainte connue : pas de métal autour de l'antenne.
- L'app Maison envoie-t-elle les couleurs en teinte/saturation ou en XY ? (Les deux sont gérés, XY via `rs-matter`.) Accepte-t-elle la lampe sans le cluster Groups (ADR-0010) ?
- Lampe finale : la C6-Zero (4 Mo) suffit-elle pour le firmware Matter ? (A priori oui : ~1,9 Mo en `--release` à l'étape 4c.)
- Les 24 Ko de la partition NVS suffisent-ils aux données Matter ? (ADR-0011)

## Idées pour plus tard

### Boîte à idées : signaux lumineux d'état

- **En attente de configuration** (premier démarrage, avant l'appairage Bluetooth) : une vague **rouge** qui va et vient le long de la barre, façon K2000 (*Larson scanner*).
- **Première connexion au Wi-Fi** (juste après la configuration Bluetooth) : la même vague, en **bleu**.
- **Wi-Fi perdu** : le **premier pixel** (3 LEDs) clignote en rouge, le reste de la barre garde ses dernières couleurs.

### Boîte à idées : pixels et ambiances

Matter (et l'app Maison) ne connaît qu'**une couleur par endpoint** : pas de dégradé ni de réglage par pixel (à revérifier avec les nouvelles versions de Matter). Zigbee n'y changerait rien (le dégradé Hue est propriétaire, Maison ne parle pas Zigbee, pas de pile Zigbee mûre en Rust). Pistes, combinables :

- **Ambiances prédéfinies** calculées dans la lampe, pixel par pixel (« Coucher de soleil », « Aurore »…), exposées chacune comme un **interrupteur Matter** en plus : utilisables dans les scènes, automatisations et avec Siri. Alternative : le cluster *Mode Select*, si l'app Maison l'affiche (à vérifier).
- **Effets autour de la couleur choisie** dans Maison (dégradé, vague, respiration), dans `led_core`.
- **Plusieurs segments = plusieurs endpoints** (ex. 3 zones de 10 pixels) : natif, mais 3 tuiles pour une barre.
- **Canal à nous** pour le contrôle fin : page web sur la lampe, ou le projet master/slave.
- Préalable commun : une fonction de rendu « état → 30 pixels » dans `led_core`.

### Autres idées

- Projet séparé **master/slave** : un master sert de pont (et de *pont Matter*), les slaves le suivent en ESP-NOW. Réutilise `led_core` et `ws2805`.
- Matter sur Thread.
- Effets dynamiques et dégradés le long de la barre (façon Hue Gradient).
- **Hors de ce projet** (qui s'arrête à la v1 avec la bande WS2805) : une bande RGBCW maison, avec **une puce par LED** (puces WS2805 seules et LED 5050 « 5 en 1 » sur un circuit imprimé à dessiner). Aucune bande du commerce ne le propose. Point à vérifier : la tension (en 12 V, une puce pour une seule LED chauffe). `led_core` et `ws2805` resserviraient tels quels.
