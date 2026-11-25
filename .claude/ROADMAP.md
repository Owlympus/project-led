# Feuille de route

**Statut actuel :** la carte est arrivée (2026-11-19) : le firmware tourne, la lampe est appairée avec l'app Maison et se pilote depuis l'iPhone (allumage, luminosité, ambiances). Toutes les étapes jusqu'à 4f sont codées ; ce qui touche à la bande attend son matériel.
**Prochaine étape :** continuer les tests sur la carte (retour au dernier état après un redémarrage, maintien du bouton 5 s, sélecteur de couleur, perte du Wi-Fi), puis brancher la bande. Bande, alimentation et 74AHCT125 commandés (2026-11-24), bouton TTP223 testé (2026-11-25) ; il manque encore la résistance de 330 Ω, le condensateur de 1000 µF et les fiches JST SM.

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
  - [x] Premier flash sur la carte (2026-11-19) : ESP32-C6 v0.2, **8 Mo** confirmés ; `cargo run` démarre sans erreur
  - [x] Code pour la LED RGB de la carte (WS2812 sur GPIO8) via le RMT : `firmware/src/board_led.rs`, vérifiée sur la carte (2026-11-19)
  - [x] LED de la carte : remplace le tour de la roue chromatique par une **copie de la bande** résumée en une couleur (`frame_preview`, `rgb_preview`), pour tester sans bande, avec une respiration au rythme des vagues pour les signaux d'état (2026-11-19) ; à voir sur la carte
  - [x] Encodage WS2805 testé sur Mac : pixel → octets → bits → impulsions (`ws2805::encode`, `ws2805::timing`)
  - [x] Code de la bande : `firmware/src/strip.rs` (GPIO2, 30 pixels), avec un test des 5 canaux ; d'abord avec le RMT, puis en SPI2 + DMA (ADR-0017, image de 840 octets)
  - [ ] Sur la vraie bande : lancer le test des canaux, vérifier l'ordre (RGB, W1/W2) et le nombre de pixels
- [x] **3. Wi-Fi**
  - [x] Connexion au réseau 2,4 GHz, reconnexion, DHCP (`esp-radio` + `embassy-net`). D'abord avec des identifiants lus à la compilation (ADR-0008), puis repris par Matter (ADR-0009) : `wifi.rs` a été retiré.
  - [ ] (optionnel) Petite page web de test / débogage
- [ ] **4. Matter** (lampe visible dans l'app Maison, ADR-0009)
  - [x] **4a.** `rs-matter-embassy` intégré : appairage par QR code via Bluetooth, Wi-Fi fourni par l'iPhone, lampe **On/Off** qui allume la bande en blanc 3000 K avec un fondu (`firmware/src/matter.rs`) ; compilé, pas encore testé sur la carte
  - [x] Sur la vraie carte : appairage avec l'app Maison et allumage / extinction depuis l'iPhone (2026-11-19)
  - [x] **4b.** Level Control et Color Control (luminosité, teinte/saturation, température), reliés à `led_core`
    - [x] `led_core::LightState` : allumage, luminosité, couleur (teinte/saturation ou température) → pixel, et conversions des échelles Matter (`matter_scale`)
    - [x] `matter.rs` : lampe *Extended Color Light* (Level Control, Color Control, Identify, Scenes) ; la bande suit un `LightState` ; compilé, pas encore testé sur la carte
    - [x] Transitions faites par `rs-matter`, lissées par la bande (ADR-0010 ; 200 ms depuis le 2026-11-20, `SMOOTHING_MS`)
  - [ ] Sur la vraie carte : luminosité, couleurs et blancs depuis l'app Maison
  - [x] **4c.** Mémorisation de l'appairage et du dernier état en flash : partition NVS, `SeqMapKvBlobStore` (ADR-0011)
  - [x] Sur la vraie carte : la lampe reste appairée après un redémarrage et après un reflash (2026-11-25 : 2 *fabrics* relues, abonnement repris, connexion à l'Apple TV en ~7 s)
  - [ ] Sur la vraie carte : la lampe revient à son dernier état (allumée, couleur, luminosité) après un redémarrage
  - [x] **4d.** Bouton tactile local (TTP223 sur GPIO3) : appui = bascule via Matter, maintien 5 s au démarrage = oubli de l'appairage (ADR-0012)
  - [x] Sur la vraie carte : TTP223 branché (3V3, GND, GPIO3), un appui bascule la lampe et l'app Maison suit (2026-11-25)
  - [ ] Sur la vraie carte : maintien 5 s au démarrage (oubli de l'appairage)
  - [x] **4e.** Signaux d'état (voir la boîte à idées, ADR-0013)
    - [x] `led_core::effect` : `Scanner` (vague K2000), `blink_on`, `StatusSignal` (attente d'appairage, connexion au Wi-Fi, Wi-Fi perdu) ; testés sur Mac
    - [x] `firmware` : envoyer une image pixel par pixel à la bande (`Strip::show`)
    - [x] `firmware` : savoir quand Matter attend l'appairage (`has_fabrics`), rejoint le Wi-Fi ou l'a perdu (tâche utilisateur `WifiWatcher`), et afficher le signal ; compilé, pas encore testé
  - [x] Premier essai : le Wi-Fi était mal détecté (tâche utilisateur lancée dès le démarrage) ; corrigé, et vague bleue revue en « connecté » d'1 s (ADR-0015)
  - [ ] Sur la vraie carte avec la bande : vague rouge avant l'appairage, aller-retour bleu une fois connecté, alerte en coupant le Wi-Fi
  - [x] **4f.** Ambiances (voir la boîte à idées, ADR-0014)
    - [x] Exposition choisie : **un interrupteur Matter par ambiance** ; premières ambiances : Coucher de soleil, Aurore boréale, Feu de camp
    - [x] `led_core::ambiance` : les trois ambiances, dessinées pixel par pixel ; outils `gradient_at`, `ping_pong`, `smooth_noise`, `Rgbcct::lerp` ; testés sur Mac
    - [x] `firmware` : un interrupteur Matter par ambiance (endpoints 2 à 4), règles avec la lampe principale, fondu d'image de 0,8 s ; compilé, pas encore testé
  - [ ] Sur la vraie carte : les tuiles dans l'app Maison, leur renommage, les règles (une seule ambiance, couleur qui la quitte…)
- [ ] **5. Boîtier** : tube acrylique, diffusion, dissipation thermique

## Relecture du code (après les premiers essais)

- [x] 1. Ménage des logs et rustines de la chasse au bug de la LED
- [x] 2. La scène dans `led_core` (`Scene<N>`), testée sur le Mac
- [x] 3. Lenteur de l'envoi à la bande (~100 ms par image) : la bande passe au SPI + DMA (ADR-0017) ; 3,5 ms mesurées sur la carte
- [x] 4. Relecture du reste : `matter.rs` découpé en un dossier `matter/` de 9 petits modules, quelques simplifications

## Questions ouvertes

- **Lampe « injoignable » après un redémarrage** (le temps que l'Apple TV se réabonne) : raccourci par `persistent-subscriptions` et `case-resumption` (2026-11-20) : la lampe reprend l'abonnement au démarrage (`Resumed persisted subscription`) et la connexion avec l'Apple TV revient **~8 s** après le démarrage, contre 1,5 à 2 min avant. L'afficher sur la lampe n'est pas possible aujourd'hui : `rs-matter` garde la table des abonnements privée (`MatterStack::state`). Piste : proposer un accès en lecture en amont.

- **Envoi à la bande très lent** (mesuré sur la carte le 2026-11-20) : ~100 ms par image avec le RMT, dont la mémoire de 48 impulsions était rechargée par notre tâche. Corrigé en passant au SPI + DMA (ADR-0017) : 3,5 ms par image mesurées ; à vérifier avec la vraie bande que le WS2805 accepte les timings (un analyseur logique sur GPIO2 permettrait de le voir avant).

- **Sélecteur de couleur de l'app Maison** : un clic au bord du spectre ramène le curseur vers le centre (essai du 2026-11-19). La lampe reçoit bien la **saturation maximale** (254) : c'est l'app qui replace son curseur. (La LED qui ne suivait pas les couleurs était un autre bug, corrigé le 2026-11-20 : `fade_done`.)
- **Appairage perdu après un reflash** (essai du 2026-11-19 : *fabric* non relue, « Device is not commissioned yet ») : **pas reproduit** le 2026-11-25. Reflash avec `ESP_LOG="info,rs_matter_embassy::persist=debug"` : 13 clés en flash, les 2 *fabrics* relues (`Loaded fabric 1` et `2`), « Device is already commissioned », abonnement repris. Cause du 19 non retrouvée ; hypothèse : un appairage dont la fin n'avait pas été validée (le Wi-Fi est enregistré tout de suite, la *fabric* seulement à la fin). Si ça revient : relancer avec ces logs (`docs/mise-en-route.md`, section 9).
- **Interrupteurs d'ambiance lents à s'éteindre dans l'app Maison** (3-4 s, essai du 2026-11-21) quand c'est la lampe qui les éteint (autre ambiance, nouvelle couleur). Côté lampe, `rs-matter` envoie le rapport sans attendre (`min_int_secs: 0`) et la veille du Wi-Fi est coupée. **Mesuré avec les logs horodatés** : l'Apple TV accuse réception du rapport « interrupteur éteint » moins de 0,1 s après la commande. Et un appui sur le bouton tactile (changement venu de la lampe, sans action dans l'app) s'affiche **instantanément** dans l'app Maison (2026-11-25) : le relais Apple TV → iPhone est donc rapide. Hypothèse : l'app Maison garde son propre affichage quelques secondes après une commande, avant d'accepter les mises à jour de l'accessoire touché. Rien à faire côté firmware.
- **Noms dans l'app Maison** : l'accessoire s'appelle « Matter Accessory » et ses 4 lumières « Lumière » à « Lumière 4 ». L'app Maison semble ignorer les noms d'endpoint de Matter (Fixed Label / User Label) : les renommer à la main pour l'instant.

- Bande reçue : consommation réelle en W/m ? (annoncée : 15 W/m, alimentation de 36 W commandée)
- Sur la bande reçue : W1 = blanc chaud ou blanc froid ? (`WhiteOrder`) Températures des blancs : 3000 K / 6500 K d'après l'annonce (`STRIP_WHITE_CHANNELS`) ; BIN du premier pixel à relier à GND ? (fiche technique)
- Génération de l'Apple TV 4K (Thread possible ou non) ?
- Boîtier pas encore défini (tube acrylique, pièces imprimées en 3D, lest dans le pied…). Seule contrainte connue : pas de métal autour de l'antenne.
- L'app Maison envoie-t-elle les couleurs en teinte/saturation ou en XY ? (Les deux sont gérés, XY via `rs-matter`.) Accepte-t-elle la lampe sans le cluster Groups (ADR-0010) ?
- Lampe finale : la C6-Zero (4 Mo) suffit-elle pour le firmware Matter ? (A priori oui : 2,08 Mo en `--release` le 2026-11-25.)
- ~~Les 24 Ko de la partition NVS suffisent-ils aux données Matter ?~~ Oui (2026-11-25) : ~3 Ko de données pour 13 clés, dont les 2 *fabrics* (1155 et 1170 octets) et l'abonnement (526 octets). (ADR-0011)

## Idées pour plus tard

### Boîte à idées : signaux lumineux d'état

Fait (étape 4e). Revu après le premier essai (ADR-0015) : la vague bleue devient un aller-retour d'1 s quand tout est connecté.

- **En attente de configuration** (premier démarrage, avant l'appairage Bluetooth) : une vague **rouge** qui va et vient le long de la barre, façon K2000 (*Larson scanner*).
- **Première connexion au Wi-Fi** (juste après la configuration Bluetooth) : la même vague, en **bleu**.
- **Wi-Fi perdu** : le **premier pixel** (3 LEDs) clignote en rouge, le reste de la barre garde ses dernières couleurs.

### Boîte à idées : pixels et ambiances

Matter (et l'app Maison) ne connaît qu'**une couleur par endpoint** : pas de dégradé ni de réglage par pixel (à revérifier avec les nouvelles versions de Matter). Zigbee n'y changerait rien (le dégradé Hue est propriétaire, Maison ne parle pas Zigbee, pas de pile Zigbee mûre en Rust). Pistes, combinables :

- **(Fait : étape 4f, ADR-0014.)** **Ambiances prédéfinies** calculées dans la lampe, pixel par pixel (« Coucher de soleil », « Aurore »…), exposées chacune comme un **interrupteur Matter** en plus : utilisables dans les scènes, automatisations et avec Siri. Alternative : le cluster *Mode Select*, si l'app Maison l'affiche (à vérifier).
- **Effets autour de la couleur choisie** dans Maison (dégradé, vague, respiration), dans `led_core`.
- **Plusieurs segments = plusieurs endpoints** (ex. 3 zones de 10 pixels) : natif, mais 3 tuiles pour une barre.
- **Canal à nous** pour le contrôle fin : page web sur la lampe, ou le projet master/slave.
- Préalable commun : une fonction de rendu « état → 30 pixels » dans `led_core`.

### Autres idées

- Projet séparé **master/slave** : un master sert de pont (et de *pont Matter*), les slaves le suivent en ESP-NOW. Réutilise `led_core` et `ws2805`.
- Matter sur Thread.
- Effets dynamiques et dégradés le long de la barre (façon Hue Gradient).
- **Hors de ce projet** (qui s'arrête à la v1 avec la bande WS2805) : une bande RGBCW maison, avec **une puce par LED** (puces WS2805 seules et LED 5050 « 5 en 1 » sur un circuit imprimé à dessiner). Aucune bande du commerce ne le propose. Point à vérifier : la tension (en 12 V, une puce pour une seule LED chauffe). `led_core` et `ws2805` resserviraient tels quels.
