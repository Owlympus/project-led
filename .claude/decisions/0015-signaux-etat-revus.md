# ADR-0015 : Signaux d'état revus après le premier essai sur la carte

- **Statut :** Accepté
- **Date :** 2026-11-19
- **Remplace :** [ADR-0013](0013-signaux-etat.md)

## Contexte
Premier essai sur la vraie carte (2026-11-19) : l'appairage avec l'app Maison fonctionne, mais les logs montrent que l'ADR-0013 reposait sur une hypothèse fausse. `rs-matter` lance la **tâche utilisateur dès le démarrage** (« concurrent commissioning mode »), pas seulement quand le Wi-Fi est en place. Notre drapeau « Wi-Fi présent » passait donc à vrai avant même l'appairage. Résultat : la vague bleue et l'alerte « Wi-Fi perdu » n'auraient jamais pu s'afficher.

Après ce test, le propriétaire a aussi revu ce que doit dire la vague bleue : plutôt « tout est connecté » que « je me connecte ».

## Décision
- **Wi-Fi présent ou non** : la tâche utilisateur (`WifiWatcher`) lit l'**état réel** de l'interface réseau que `rs-matter` lui passe (`NetifDiag::netifs`, champ `operational`). Elle se réveille à chaque changement (`NetChangeNotif::wait_changed`). Si la tâche est arrêtée, un `Drop` considère le Wi-Fi perdu.
- **Signaux** :

  | Situation | Signal |
  |---|---|
  | Pas appairée (en attente de synchronisation) | vague **rouge**, en continu |
  | Vient d'être appairée **et** connectée au Wi-Fi (après l'appairage, au démarrage, au retour du Wi-Fi) | **un aller-retour bleu** rapide, en 1 s, puis rien |
  | Appairée, Wi-Fi venu puis perdu | **premier pixel** qui clignote en rouge |
  | Appairée, Wi-Fi en cours de connexion après la mise sous tension | rien (la couleur de la lampe) |
  | Tout va bien | rien |

- `StatusSignal::JoiningWifi` devient `StatusSignal::Connected`, avec sa durée (`CONNECTED_MS`, 1 s = un aller-retour).
- « Appairée » se lit toujours avec `Matter::has_fabrics()`, et `watch_status` interroge toujours 4 fois par seconde : ces deux points de l'ADR-0013 ont tenu.

## Alternatives envisagées
- **Vague bleue pendant la connexion** (ADR-0013) : la connexion est rapide (une à deux secondes dans les logs), et un « c'est bon » est plus utile qu'un « j'essaie ».
- **Se fier au démarrage et à l'arrêt de la tâche utilisateur** (ADR-0013) : démenti par les logs.

## Conséquences
- À chaque démarrage normal : couleur de la lampe, puis un aller-retour bleu dès que le Wi-Fi est là.
- À vérifier sur la carte : le passage à « non opérationnel » quand le Wi-Fi tombe (couper la box ou s'éloigner), donc l'alerte sur le premier pixel.
