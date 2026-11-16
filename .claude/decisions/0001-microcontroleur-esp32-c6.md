# ADR-0001 : Microcontrôleur ESP32-C6

- **Statut :** Accepté
- **Date :** 2026-11-16

## Contexte
Il faut un microcontrôleur pas cher, programmable en Rust, capable de piloter une bande LED et d'être contrôlé depuis un iPhone. Le protocole de pilotage n'était pas encore fixé au moment du choix.

## Décision
**ESP32-C6** (RISC-V, Wi-Fi 6 en 2,4 GHz, Bluetooth LE 5, radio 802.15.4 pour Thread/Zigbee).
De préférence une carte avec **8 Mo de flash** (ex. ESP32-C6-DevKitC-1), car `rs-matter` est volumineux.

## Alternatives envisagées
- **ESP32-C3** : moins cher, suffisant pour Matter sur Wi-Fi, mais pas de Thread. Écarté pour garder toutes les options ouvertes.

## Conséquences
- Matter sur Wi-Fi **et** sur Thread possibles.
- Bon support en Rust via `esp-hal` (Wi-Fi/BLE via `esp-radio`, périphérique RMT pour la bande).
- Le Bluetooth LE sert à l'appairage Matter avec l'iPhone.
