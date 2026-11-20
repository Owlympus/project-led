# ADR-0010 : Lampe couleur Matter, transitions faites par `rs-matter`

- **Statut :** Accepté
- **Date :** 2026-11-17

## Contexte
L'étape 4b ajoute la luminosité et la couleur à la lampe On/Off de l'ADR-0009. `rs-matter` fournit les clusters Level Control et Color Control complets. Ils gardent eux-mêmes les valeurs et exécutent les transitions demandées par l'app Maison : une transition arrive chez nous comme une suite de petits pas (un niveau à la fois pour la luminosité, toutes les 100 ms pour la couleur). Notre firmware faisait déjà un fondu de 400 ms à l'allumage et à l'extinction.

Il fallait aussi choisir le type d'appareil Matter : la bande a du RGB et deux blancs.

## Décision
- Type d'appareil **Extended Color Light**, avec les trois modes de couleur : teinte/saturation, **XY** (exigé par ce type d'appareil) et température. Le mode XY est converti en teinte/saturation par `rs-matter` (`SetDeviceColor::to_hue_saturation`) : pas de conversion XY à nous pour l'instant.
- **Les transitions sont faites par `rs-matter`** : c'est lui qui suit les durées demandées par l'app Maison. L'allumage et l'extinction passent par un fondu de 0,4 s (`ON_OFF_TRANSITION_TIME`), lui aussi exécuté par `rs-matter` via la luminosité.
- Notre tâche de la bande se contente d'un **lissage de 100 ms** entre deux pas reçus, pour que les marches ne se voient pas. Elle repart toujours de la couleur affichée à l'instant, donc un nouveau pas en plein lissage ne crée pas de saut.
- Les trois « hooks » mettent à jour un état unique `led_core::LightState` (partagé dans un `Mutex` bloquant), puis réveillent la tâche de la bande par un `Signal`.
- Plage de blancs annoncée à l'app Maison = celle des deux LED blanches de la bande (`STRIP_WHITE_CHANNELS`, 2700 K à 6500 K supposés).
- Clusters de l'endpoint : Descriptor, Identify, On/Off, Level Control, Color Control et Scenes Management. **Groups est laissé de côté** : il demande la fonctionnalité `groups` de `rs-matter` (multidiffusion, plus de mémoire), et l'app Maison ne devrait pas en avoir besoin. À ajouter si l'appairage ou une scène échoue à cause de lui.

## Alternatives envisagées
- **Garder nos propres transitions** et ne prendre que la valeur finale : impossible proprement, `rs-matter` n'expose que les pas successifs, et les durées demandées par l'app ne seraient pas respectées.
- **Aucun lissage de notre côté** : plus simple, mais les pas de couleur toutes les 100 ms risquent de se voir.
- **Color Temperature Light** (blancs seuls) ou **Dimmable Light** : perdent les couleurs.

## Conséquences
- `LightState` sert de lien entre Matter et la bande. Il servira aussi aux ambiances et aux effets de la boîte à idées.
- Le firmware passe à ~1,9 Mo en `--release` : il tient toujours dans les 4 Mo de la C6-Zero.
- Réserve de mémoire des futures de Matter portée à 32 Ko (+ Bluetooth), valeur de l'exemple de `rs-matter`. Si la pile panique au démarrage, l'augmenter.
- À vérifier sur la vraie carte : le comportement de l'app Maison (quel mode de couleur elle envoie, absence de Groups), et la douceur des transitions.
- `led_core::light::matter_scale::hue` n'est pas utilisé : `rs-matter` donne directement la teinte « étendue » sur 16 bits.

## Note (2026-11-20)
Le lissage de la bande est passé de 100 à 200 ms (`SMOOTHING_MS`) après les essais sur la carte : un simple réglage, la décision reste la même.
