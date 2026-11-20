# ADR-0014 : Un interrupteur Matter par ambiance, et leurs règles avec la lampe

- **Statut :** Accepté (règle 3 remplacée par l'[ADR-0016](0016-quitter-une-ambiance.md))
- **Date :** 2026-11-17

## Contexte
`led_core::Ambiance` dessine trois ambiances pixel par pixel : Coucher de soleil, Aurore boréale, Feu de camp. Matter et l'app Maison ne connaissent qu'une couleur par lampe. Il a été décidé (CHANGELOG, 2026-11-17) d'exposer **chaque ambiance comme un interrupteur Matter** sur la même lampe. Reste à fixer comment ces interrupteurs s'articulent avec la lampe principale.

## Décision
- **Endpoints 2, 3 et 4**, dans l'ordre de `Ambiance::ALL`, de type **On/Off Light**, avec les clusters Descriptor, Identify et On/Off. L'app Maison les montre comme des lampes de plus dans l'accessoire « Lampe barre ». Leurs noms seront à donner dans l'app : Matter n'en transmet pas pour ces endpoints.
- `LightState` porte l'ambiance (`ambiance: Option<Ambiance>`). La bande affiche l'ambiance à la **luminosité de la lampe principale**.
- **Règles :**
  1. Allumer une ambiance l'affiche, et **éteint l'interrupteur de l'ambiance précédente** : une seule à la fois.
  2. Allumer une ambiance quand la lampe est éteinte **allume la lampe**, via Matter, donc avec le fondu, et l'app Maison suit.
  3. Éteindre l'interrupteur de l'ambiance affichée revient à la **couleur unie**. La lampe reste allumée.
  4. **Changer la couleur** de la lampe dans l'app quitte l'ambiance : son interrupteur s'éteint.
  5. **Éteindre la lampe** quitte aussi l'ambiance.
  6. Après une **coupure de courant**, les ambiances repartent éteintes (`StartUpOnOff = Off`) : la lampe revient dans sa couleur unie.
- Pour afficher « éteint » dans l'app sans relancer la logique, les interrupteurs reçoivent `OutOfBandMessage::Update`. Il change seulement l'état annoncé, sans rappeler nos hooks, donc sans boucle.
- Passer d'une image à l'autre (couleur ↔ ambiance, ou ambiance ↔ ambiance) se fait en **fondu de 0,8 s**, image entière (`led_core::effect::crossfade`). Les petits pas de Matter restent lissés sur 100 ms (ADR-0010).

## Alternatives envisagées
- **Cluster Mode Select** (une liste de modes) : plus propre, mais on ne sait pas si l'app Maison l'affiche.
- **Type d'appareil « prise » (On/Off Plug-in Unit)** : l'app Maison les montrerait comme des prises, moins parlant pour des lumières.
- **Garder l'ambiance après une coupure** : il faudrait ordonner la restauration des clusters au démarrage pour que la couleur restaurée ne l'efface pas. C'est plus fragile, pour un gain faible.
- **La luminosité de l'ambiance réglée sur son propre interrupteur** : un On/Off n'a pas de luminosité, et une seule luminosité est plus simple à comprendre.

## Conséquences
- 3 tuiles de plus dans l'app Maison. On pourra les ranger ou les masquer dans un groupe.
- Les ambiances marchent dans les scènes, les automatisations et avec Siri (« allume Feu de camp »), une fois renommées.
- Ajouter une ambiance = une variante dans `Ambiance`, un endpoint et un signal de plus.
- **À vérifier sur la vraie carte** :
  - l'affichage des tuiles dans l'app Maison et leur renommage ;
  - que la commande « tout éteindre » de l'app éteint bien la lampe ;
  - que la règle 4 ne se déclenche pas toute seule : elle ne réagit qu'à une couleur **différente** de la couleur enregistrée.
