# ADR-0008 : Identifiants Wi-Fi lus à la compilation (provisoire)

- **Statut :** Remplacé par [ADR-0009](0009-matter-avec-rs-matter-embassy.md) (les identifiants Wi-Fi arrivent par l'appairage Matter)
- **Date :** 2026-11-17

## Contexte
À l'étape 3, la lampe doit rejoindre le Wi-Fi de la maison. Il lui faut le nom du réseau (SSID) et le mot de passe. Ils ne doivent **jamais** finir dans git : le dépôt est sur GitHub.

À terme, c'est **Matter** qui fournira ces identifiants : pendant l'appairage, l'iPhone les transmet à la lampe par Bluetooth (étape 4). La solution de l'étape 3 n'a donc besoin que de tenir jusque-là.

## Décision
Le firmware lit `WIFI_SSID` et `WIFI_PASSWORD` **dans les variables d'environnement, au moment de la compilation** (`option_env!`). On les définit dans son propre terminal (ou dans `~/.zshrc`) avant `cargo run`. Sans ces variables, le firmware compile et tourne quand même, sans Wi-Fi, avec un avertissement dans les logs.

Authentification : WPA2 personnel (accepté aussi par les box en mode WPA2/WPA3), ou réseau ouvert si le mot de passe est vide.

## Alternatives envisagées
- **Fichier de secrets ignoré par git** (`wifi.toml`…) : plus de code (lecture dans `build.rs`), et le risque de commit par erreur existe toujours.
- **Mode point d'accès + page web de configuration** : beaucoup de travail pour une solution que Matter remplacera.
- **Identifiants stockés en flash** : utile seulement avec un moyen de les saisir, donc avec l'une des deux solutions ci-dessus.

## Conséquences
- Le mot de passe se retrouve en clair dans le binaire flashé : acceptable pour une lampe à la maison, mais il ne faut pas partager ce binaire.
- Changer de réseau demande de recompiler et de reflasher.
- Les réseaux **WPA3 uniquement** ne sont pas pris en charge par `esp-radio` 1.0 bêta.
- Cette décision sera remplacée quand Matter fournira les identifiants (étape 4).
