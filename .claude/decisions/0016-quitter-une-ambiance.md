# ADR-0016 : Éteindre une ambiance éteint la lampe

- **Statut :** Accepté
- **Date :** 2026-11-20
- **Remplace :** la règle 3 de l'[ADR-0014](0014-interrupteurs-ambiance.md)

## Contexte
La règle 3 de l'ADR-0014 disait : éteindre l'interrupteur de l'ambiance affichée revient à la **couleur unie**, lampe allumée. À l'essai sur la carte (2026-11-20), le propriétaire a allumé une ambiance alors que la lampe était éteinte, puis l'a éteinte. La lampe est restée allumée, dans la dernière couleur choisie avant l'extinction, ce qui surprend.

Une première idée était de revenir à l'état d'avant l'ambiance : lampe éteinte ou couleur unie. Elle a été écartée au profit d'une règle plus simple (voir les alternatives).

## Décision
- Éteindre l'interrupteur de l'ambiance affichée **éteint la lampe**, quel que soit l'état d'avant l'ambiance. La demande passe par Matter (`LightRequest::Off`), donc avec le fondu, et l'app Maison suit.
- Pendant le fondu d'extinction, la bande garde l'ambiance, puis tout s'éteint : pas de passage par la couleur unie.
- Pour revenir à une couleur unie depuis une ambiance, on **choisit une couleur** dans l'app Maison : la règle 4 de l'ADR-0014 quitte alors l'ambiance, lampe allumée.
- Les autres règles de l'ADR-0014 sont inchangées.

## Alternatives envisagées
- **Garder la règle 3** (toujours revenir à la couleur unie) : surprenant quand on « éteint » quelque chose et que la lampe reste allumée.
- **Revenir à l'état d'avant l'ambiance** (lampe éteinte ou couleur unie) : plus fin, mais il faut retenir cet état, et le résultat est moins prévisible pour l'utilisateur. Le sélecteur de couleur est juste à côté dans l'app Maison pour revenir à une couleur unie.

## Conséquences
- Un interrupteur d'ambiance se comporte comme un interrupteur de lampe : allumé = l'ambiance, éteint = la lampe éteinte.
- En rallumant la lampe, on retrouve sa dernière couleur unie : elle reste en mémoire pendant l'ambiance.
- À vérifier sur la carte : ambiance → extinction de l'ambiance → lampe éteinte, que la lampe ait été éteinte ou allumée avant.
