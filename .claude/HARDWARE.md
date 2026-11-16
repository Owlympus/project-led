# Matériel

## Nomenclature (par lampe)

| Élément | Détail | Statut |
|---|---|---|
| Microcontrôleur | ESP32-C6 devkit (8 Mo de flash de préférence) | ✅ choisi |
| Bande LED | WS2805 RGBCCT, 12 V, 60 LEDs/m, 1,5 m, IP30 de préférence | 🔍 annonce à vérifier |
| Alimentation | 12 V, puissance = W/m × 1,5 m + 20 à 30 % de marge | ⏳ dépend de la consommation de la bande |
| Abaisseur de tension | 12 V → 5 V / 3,3 V pour alimenter l'ESP32 | ✅ acheté |
| Adaptateur de niveau | **74AHCT125** (ou SN74AHCT1G125) alimenté en 5 V, sur la ligne de données | ❌ à acheter |
| Résistance | ~330 Ω en série sur la ligne de données | ❌ à acheter |
| Condensateur | ~1000 µF / 16 V minimum, sur l'alimentation de la bande | ❌ à acheter |
| Bouton | Capteur tactile TTP223 (optionnel, pour un contrôle local) | ❌ plus tard |
| Boîtier | Tube acrylique diffusant | ❌ plus tard |

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
