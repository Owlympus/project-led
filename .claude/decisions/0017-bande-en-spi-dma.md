# ADR-0017 : Piloter la bande avec le SPI et son DMA, plus avec le RMT

- **Statut :** Accepté
- **Date :** 2026-11-20

## Contexte
Jusqu'ici, la bande était pilotée par le **RMT**, comme la LED de la carte. Sur la carte (2026-11-20), envoyer une image prenait **~100 ms** au lieu des ~1,5 ms attendues, soit ~8 images par seconde au lieu de 50.

La cause est dans `esp-hal` :
- un canal RMT ne garde que **48 impulsions** en mémoire, alors qu'une image en compte **1 201** (30 pixels × 40 bits, plus la fin) ;
- `esp-hal` recharge donc la mémoire par paquets de 24. L'interruption ne fait que **réveiller notre tâche**, et c'est la tâche qui écrit le paquet suivant ;
- 24 impulsions durent ~30 µs. Avec la pile Matter et le Wi-Fi sur le même exécuteur, la tâche passe bien plus tard, 50 fois par image.

Plus grave : en attendant, le RMT **reboucle** sur l'ancien paquet. La vraie bande aurait affiché des pixels faux, et une pause de plus de 280 µs au milieu de l'image l'aurait coupée en deux.

## Décision
- La bande passe sur le **SPI2 avec son DMA**. Le DMA envoie toute l'image d'un coup, sans le processeur ni notre tâche.
- **Encodage** (dans `ws2805::spi`, testé sur Mac) : chaque bit WS2805 devient **4 bits SPI** à **3,2 MHz** (312,5 ns par bit SPI) :

  | Bit WS2805 | Bits SPI | Haut | Bas |
  |---|---|---|---|
  | 0 | `1000` | 312,5 ns (fiche : 220 à 380) | 937,5 ns (580 à 1 000) |
  | 1 | `1100` | 625 ns (fiche : 580 à 1 000) | 625 ns (580 à 1 000) |

- Chaque image est **entourée de 300 µs à 0** (120 octets), **avant** et **après** :
  - avant : la bande part d'une image propre, quel que soit le niveau que le SPI a laissé sur la ligne entre deux images ;
  - après : la bande affiche l'image.

  Tampon : 120 + 30 × 20 + 120 = **840 octets** ; envoi : **~2,6 ms**.
- Le tampon de travail du DMA (1 024 octets) contient une image entière : elle n'est jamais envoyée en deux morceaux.
- Même broche (**GPIO2**, sortie MOSI) : le câblage ne change pas.
- La **LED de la carte reste sur le RMT** : ses 25 impulsions tiennent d'un coup dans sa mémoire, sans rechargement.

## Alternatives envisagées
- **RMT bloquant** : la tâche recharge le RMT dans une boucle serrée (~1,5 ms par image). Changement plus petit, mais Matter est bloqué 1,5 ms toutes les 20 ms, et une interruption du Wi-Fi au mauvais moment fausserait encore des pixels.
- **RMT avec plus de mémoire** (la mémoire des autres canaux, jusqu'à ~144 impulsions) : toujours très loin de 1 201.
- **Périphérique I2S** : possible aussi avec DMA, mais plus compliqué à régler que le SPI pour le même résultat.

## Conséquences
- 50 images par seconde possibles, et un processeur libre pendant l'envoi.
- `ws2805` gagne un module `spi`. Les impulsions (`pulses`, `Timing`) restent pour la LED de la carte.
- Un log au démarrage donne la durée d'envoi d'une image (`strip: one frame sent in … µs`). **Mesuré sur la carte (2026-11-20) : 3,5 ms** (calcul, copie et lancement du DMA compris), contre ~100 ms avec le RMT. Lissage nettement plus fluide sur la LED de la carte.
- **À vérifier avec la vraie bande** : que le WS2805 accepte bien ces timings, et le test des canaux. Un analyseur logique ou un oscilloscope sur GPIO2 permettrait de voir le signal avant même d'avoir la bande.
