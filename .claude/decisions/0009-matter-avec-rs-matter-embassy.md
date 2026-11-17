# ADR-0009 : Matter avec `rs-matter-embassy`, lampe d'abord en On/Off

- **Statut :** Accepté
- **Date :** 2026-11-17

## Contexte
L'ADR-0004 fait de chaque lampe un appareil Matter. Il faut une implémentation de Matter qui tourne en Rust `no_std` sur l'ESP32-C6, avec Embassy et nos versions d'`esp-hal` (1.2) et d'`esp-radio` (1.0 bêta).

## Décision
- **`rs-matter`** (implémentation Rust officielle du projet Matter) et **`rs-matter-embassy`** (son adaptation à Embassy : Bluetooth pour l'appairage, Wi-Fi, réseau).
- Pas encore publiées sur crates.io dans une version utilisable : **dépendances Git figées sur un commit** (`rev = "…"`), et `[patch.crates-io]` pour que `rs-matter-embassy` utilise la même version de `rs-matter`. Mettre à jour = changer ces deux `rev` ensemble.
- **Appairage par Bluetooth** : l'iPhone transmet les identifiants Wi-Fi à la lampe. Cela **remplace l'ADR-0008** et le module `wifi.rs`.
- **Certificats de test** de Matter (vendeur `0xFFF1`, code `20202021`) : l'app Maison signale un « accessoire non certifié », qu'on accepte. Un vrai certificat demande une adhésion payante à la CSA : hors de propos pour un projet perso.
- Hôte Bluetooth **`trouble`** (100 % Rust) plutôt que NimBLE (C, demande clang).
- Premier jalon (4a) : une lampe **On/Off** seulement. Luminosité, couleur et température viendront ensuite (4b), puis la mémorisation en flash (4c).

## Alternatives envisagées
- **ESP-IDF + `esp-idf-matter`** (Rust au-dessus du SDK C d'Espressif) : plus éprouvé, mais il faudrait abandonner `no_std`, esp-hal et Embassy (ADR-0003), donc réécrire le firmware.
- **SDK Matter C++ d'Espressif (`esp-matter`)** : la référence industrielle, mais en C++, hors de l'objectif d'apprendre Rust.

## Conséquences
- Projet jeune : l'auteur prévient que l'appairage peut avoir des ratés. On le découvrira au premier test sur la carte.
- Le firmware passe à ~1,8 Mo en `--release` (~2,3 Mo en debug) : il tient dans 4 Mo, donc a priori sur la C6-Zero.
- Tas porté à 100 Ko (pilote radio + Matter), pile Matter (~35 à 50 Ko) dans un `static`.
- Tant que rien n'est sauvegardé en flash (4c), il faut retirer la lampe de l'app Maison et la réappairer après chaque redémarrage.
