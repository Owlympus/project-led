# ADR-0013 : Signaux d'état : comment le firmware sait quoi afficher

- **Statut :** Remplacé par [ADR-0015](0015-signaux-etat-revus.md)
- **Date :** 2026-11-17

## Contexte
`led_core::StatusSignal` sait dessiner trois signaux : attente d'appairage (vague rouge), connexion au Wi-Fi (vague bleue) et Wi-Fi perdu (premier pixel qui clignote). Le firmware doit savoir **quand** afficher chacun. Ces informations sont à l'intérieur de `rs-matter`, qui n'offre pas d'événement « appairé » ou « Wi-Fi perdu » tout prêt.

## Décision
- **Appairée ou non** : `Matter::has_fabrics()`. Une *fabric* est un réseau Matter dont la lampe fait partie, ici celui de l'Apple TV. Pas de fabric = pas appairée.
- **Wi-Fi présent ou non** : une **tâche utilisateur** de `rs-matter` (`UserTask`, le dernier argument de `run_wifi`). `rs-matter` la démarre quand le Wi-Fi est en place (avec une adresse) et l'arrête quand il tombe. Notre tâche (`WifiWatcher`) lève un drapeau à son démarrage. Un objet `WifiUp` le baisse dans son `Drop`, appelé quand `rs-matter` abandonne la tâche.
- Une petite boucle (`watch_status`), lancée à côté de la pile Matter avec `select`, regarde ces informations **4 fois par seconde**, choisit le signal et ne prévient la bande que s'il change (`STATUS_CHANGED`).
- Choix du signal :

  | Situation | Signal |
  |---|---|
  | Pas de fabric | vague **rouge** |
  | Appairée, Wi-Fi pas encore venu depuis la mise sous tension | vague **bleue** |
  | Appairée, Wi-Fi venu puis perdu | **premier pixel** qui clignote en rouge |
  | Appairée, Wi-Fi présent | rien (la couleur de la lampe) |

- **Écart avec la boîte à idées** : la vague bleue s'affiche à **chaque** démarrage tant que le Wi-Fi n'est pas là, pas seulement à la toute première connexion. C'est plus simple, et le message reste vrai (« je me connecte »). Après une coupure de courant, la lampe montre donc quelques secondes de bleu avant sa couleur.
- La tâche de la bande dessine une **image complète** (30 pixels) à chaque trame : la couleur de la lampe, puis le signal par-dessus. Elle ne dort que si rien ne bouge, ni transition ni signal.

## Alternatives envisagées
- **Lire l'état interne du Wi-Fi** de `rs-matter` (`NetCtlState`) : pas exposé simplement par `rs-matter-embassy`.
- **Attendre des événements** plutôt que d'interroger 4 fois par seconde : `rs-matter` n'en publie pas pour les fabrics. L'interrogation est simple et coûte très peu.
- **Vague bleue seulement à la première connexion** (comme dans la boîte à idées) : il faudrait retenir en flash que la lampe s'est déjà connectée une fois, pour un gain faible.

## Conséquences
- La bande anime en continu tant qu'un signal est affiché, à 50 images par seconde. C'est sans conséquence pour la puce.
- La lampe signale visuellement son état dès le premier branchement, sans Mac : utile pour la tester et pour l'appairer.
- **À vérifier sur la vraie carte** :
  - que `rs-matter` arrête bien la tâche utilisateur quand le Wi-Fi tombe (sinon le signal « Wi-Fi perdu » ne s'afficherait jamais) ;
  - que la vague rouge s'arrête bien à la fin de l'appairage. Elle reste visible pendant l'appairage lui-même, puisque la fabric n'existe qu'à la fin : c'est voulu.
