# ADR-0012 : Bouton tactile TTP223 : appui = bascule via Matter, maintien au démarrage = oubli de l'appairage

- **Statut :** Accepté
- **Date :** 2026-11-17

## Contexte
L'étape 4d ajoute un contrôle local à la lampe, sans iPhone. Le matériel prévu est un capteur tactile **TTP223** (`HARDWARE.md`), qui donne un simple signal numérique (haut tant qu'un doigt touche la zone, dans son mode par défaut).

Deux besoins :
- allumer et éteindre la lampe à la main, en gardant l'app Maison à jour ;
- pouvoir **oublier l'appairage** sans Mac ni `espflash` (lampe donnée, changement de réseau, appairage raté). `rs-matter` n'a pas de fonction « remise à zéro » toute faite.

## Décision
- **Appui** : bascule allumé / éteint. La demande passe par le cluster On/Off de Matter (`OnOffHooks::run` et `OutOfBandMessage::Toggle`), et pas directement par la bande. Ainsi :
  - l'app Maison voit le changement ;
  - le fondu d'allumage et d'extinction s'applique ;
  - le nouvel état est sauvegardé en flash comme les autres.
- **Maintien du bouton à la mise sous tension pendant 5 s** : la partition NVS est effacée avant le démarrage de Matter. La lampe repart comme au premier jour et affiche un nouveau QR code. Relâcher avant 5 s annule.
- Broche : **GPIO3** (provisoire, à côté de GPIO2), en entrée avec résistance de rappel vers la masse (*pull-down*). Le TTP223 est alimenté en **3,3 V**, sinon sa sortie serait en 5 V, trop pour le C6.
- Anti-rebond de 50 ms entre deux changements. La tâche attend les fronts par interruption, sans interrogation en boucle.

## Alternatives envisagées
- **Appui long pendant le fonctionnement** pour la remise à zéro : le stockage Matter possède déjà la flash. Il faudrait la partager ou redémarrer la puce avec un drapeau, ce qui est plus compliqué. Le maintien au démarrage est un geste courant sur les objets connectés.
- **Piloter la bande directement** depuis le bouton : l'app Maison ne verrait pas le changement, et l'état ne serait pas sauvegardé.
- **Appui long = variation de luminosité** : intéressant, mais à garder pour plus tard. Le geste « maintenir » reste libre pendant le fonctionnement.

## Conséquences
- Un nouveau module `button.rs` et une tâche de plus. Matter reçoit un booléen « oublier l'appairage » au démarrage.
- Bouton débranché : grâce au *pull-down*, rien ne se passe.
- Ce geste au démarrage remplace la commande `espflash erase-region`, qui reste possible.
- **Risque à vérifier en premier** : le TTP223 se calibre à sa mise sous tension. Si un doigt est déjà posé, il peut le prendre pour son état « au repos » et ne jamais signaler l'appui. Le maintien au démarrage ne marcherait alors pas. Solution de repli : un **appui très long (10 s) pendant le fonctionnement**, qui note la demande dans la mémoire RTC (elle survit à un redémarrage logiciel) puis redémarre la puce ; l'effacement se fait au démarrage suivant, comme ici.
- À vérifier sur la vraie carte : le mode du TTP223 reçu (certains modules sont réglés en « bascule » ou « actif bas » par des cavaliers à souder), et la broche GPIO3 sur la carte Waveshare.
