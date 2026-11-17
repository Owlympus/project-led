# ADR-0006 : Températures de blanc en mireds, calculs en entiers

- **Statut :** Accepté
- **Date :** 2026-11-16

## Contexte
`led_core` doit convertir une température de blanc demandée (ex. « 4000 K ») en un mélange des deux LEDs blanches de la bande (chaude ≈ 2700 K, froide ≈ 6500 K).

## Décision
- La température est stockée en **mireds** (`1 000 000 / kelvin`) dans `ColorTemperature`. Des constructeurs `from_kelvin` / `from_mireds` existent pour les deux usages.
- Le mélange chaud/froid est **linéaire en mireds**, avec un total constant (`warm + cold = 255`) ; la luminosité est appliquée ensuite par `scale`.
- Les calculs se font **en entiers** (`u16`/`u32`), sans `f32`.

## Alternatives envisagées
- **Kelvin** : plus parlant, mais un mélange linéaire en kelvin paraît inégal à l'œil, et Matter n'utilise pas cette unité.
- **Flottants** : plus simples à écrire, mais l'ESP32-C6 n'a pas d'unité de calcul flottant (émulation logicielle, plus lente) et les tests exacts sont plus délicats.

## Conséquences
- Correspondance directe avec l'attribut Matter `ColorTemperatureMireds` (cluster Color Control) à l'étape 4.
- Arrondis de quelques kelvin lors des allers-retours kelvin ↔ mireds (sans effet visible).
- Les températures réelles des LEDs de la bande sont à relever sur sa fiche technique (`WhiteChannels`).
