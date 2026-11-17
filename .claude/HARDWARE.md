# Matériel

## Nomenclature (par lampe)

| Élément | Détail | Statut |
|---|---|---|
| Microcontrôleur | **Waveshare ESP32-C6-DEV-KIT-N8** (module ESP32-C6-WROOM-1, 8 Mo de flash), voir ci-dessous | ✅ commandé |
| Bande LED | WS2805 RGBCCT, 12 V, 60 LEDs/m, 1,5 m, IP30 de préférence | 🔍 annonce à vérifier |
| Alimentation | 12 V, puissance = W/m × 1,5 m + 20 à 30 % de marge | ⏳ dépend de la consommation de la bande |
| Abaisseur de tension | 12 V → 5 V / 3,3 V pour alimenter l'ESP32 | ✅ acheté |
| Adaptateur de niveau | **74AHCT125** (ou SN74AHCT1G125) alimenté en 5 V, sur la ligne de données | ❌ à acheter |
| Résistance | ~330 Ω en série sur la ligne de données | ❌ à acheter |
| Condensateur | ~1000 µF / 16 V minimum, sur l'alimentation de la bande | ❌ à acheter |
| Bouton | Capteur tactile TTP223 (optionnel, pour un contrôle local) | ❌ plus tard |
| Boîtier | Tube acrylique diffusant | ❌ plus tard |

## Carte de développement : Waveshare ESP32-C6-DEV-KIT-N8

- Réf. Waveshare SKU 25723, part no. `ESP32-C6-DEV-KIT-N8`.
- Module **ESP32-C6-WROOM-1** : « NX » sur l'annonce et la photo désigne toute la série (X = taille de flash, N8 ou N16). La version commandée a **8 Mo**. On le vérifiera avec `espflash board-info`.
- **Brochage compatible ESP32-C6-DevKitC-1** : la doc Espressif, Wokwi et les exemples `esp-hal` s'appliquent.
- **Une seule prise USB-C**, avec un hub interne (CH334) qui donne à la fois l'USB natif de la puce (flash, logs, débogage JTAG) et un port série via un CH343.
- **Barrettes de broches à souder** (la version `-M` les a déjà soudées).
- À confirmer sur la carte reçue : LED RGB intégrée (GPIO8 sur la DevKitC), boutons Boot / Reset, type d'antenne.

### Broches

- **À éviter :**
  - GPIO4, 5, 8, 9 et 15, broches de démarrage (*strapping*) ;
  - GPIO12 et 13, USB ;
  - GPIO16 et 17, UART ;
  - GPIO0 et 1, quartz 32 kHz s'il est utilisé.
- **Candidates pour les données de la bande :** GPIO2, 3, 6, 7, 10, 11 ou 18 à 23 (choix à faire à l'étape 2).

### Alimentation pendant le développement

L'USB, la broche 5V et la broche 3V3 sont **trois alimentations mutuellement exclusives**. N'en brancher qu'une à la fois.

Pendant le dev, la carte est alimentée **par l'USB** et la bande par le 12 V. Les deux partagent **seulement la masse (GND)**. L'abaisseur ne sera relié à la broche 5V que dans la lampe finale, sans USB.

### Pour la lampe finale

- Candidate plus compacte : **Waveshare ESP32-C6-Zero** (puce C6FH4, **4 Mo** de flash). Il faudra vérifier que le firmware Matter y tient.
- Antenne : un profilé en aluminium bloquerait l'antenne intégrée. Il faut soit un tube tout acrylique, soit une antenne qui dépasse du profilé.

## Côté maison

- iPhone sous iOS 16.1 ou plus
- **Apple TV 4K** comme concentrateur (✅ en ma possession, génération à confirmer)
- Wi-Fi 2,4 GHz

## Câblage

```
12 V ──┬──────────────────────────────► bande +12 V
       │   [1000 µF entre +12 V et GND]
       └─► abaisseur ─► 5 V ─► ESP32 (5V) et 74AHCT125 (VCC)

ESP32 GPIO ─► 74AHCT125 (entrée → sortie) ─► 330 Ω ─► bande DIN

GND : COMMUNE entre l'alim, l'ESP32, le 74AHCT125 et la bande
```

## Pièges connus

- **Abaisseur de tension ≠ adaptateur de niveau.** L'abaisseur alimente l'ESP32. Le signal de données sort en 3,3 V et doit être remonté à 5 V, sinon on risque des LEDs qui clignotent de façon aléatoire.
- **Masse commune obligatoire**, sinon le signal de données n'a pas de référence.
- Le C6 ne capte que le Wi-Fi **2,4 GHz**.
- Câble USB-C **data**, pas un câble charge seule.
- Ne **jamais** alimenter la carte par l'USB et par la broche 5V en même temps.
