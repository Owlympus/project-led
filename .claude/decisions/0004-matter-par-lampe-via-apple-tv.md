# ADR-0004 : Chaque lampe est un appareil Matter appairé à l'Apple TV

- **Statut :** Accepté
- **Date :** 2026-11-16

## Contexte
Objectif : piloter les lampes depuis l'**iPhone**, comme des Hue. L'app Maison d'Apple exige un **concentrateur domestique** (Apple TV / HomePod) pour les accessoires Matter. Le propriétaire a une **Apple TV 4K** (génération à confirmer).

## Décision
Chaque lampe est **un appareil Matter autonome**, appairé directement à l'app Maison (QR code), avec l'Apple TV comme concentrateur.
**Matter sur Wi-Fi d'abord** ; Thread éventuellement plus tard.

## Alternatives envisagées
- **Lampes égales entre elles** (ESP-NOW + app web, sans concentrateur) : possible mais pas d'intégration à l'app Maison.
- **Master/slave** (une lampe sert de pont, les autres la suivent en ESP-NOW, le master s'expose comme *pont Matter*) : intéressant, **mis de côté pour un futur projet séparé**.
- **Zigbee** : écosystème Rust immature, et inutile sans pont Zigbee.

## Conséquences
- Toutes les lampes ont le même firmware.
- Groupes, scènes et automatisations sont gérés par l'app Maison, pas par le firmware.
- Matter est 100 % local : pas de cloud ni de compte.
- Thread ne sera possible que si l'Apple TV est une 4K 2ᵉ gén. ou une 3ᵉ gén. 128 Go (à vérifier dans Réglages → Général → Informations).
- À prévoir quand même : bouton tactile sur la lampe et mémorisation du dernier état après une coupure.
