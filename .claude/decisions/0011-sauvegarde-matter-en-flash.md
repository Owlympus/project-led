# ADR-0011 : Données Matter sauvegardées dans la partition NVS

- **Statut :** Accepté
- **Date :** 2026-11-17

## Contexte
Jusqu'à l'étape 4b, Matter utilisait un stockage factice (`DummyKvBlobStore`) : après chaque redémarrage, la lampe oubliait son appairage et il fallait la supprimer de l'app Maison puis la réappairer. Il faut garder en flash :
- l'appairage (certificats de la « fabric » de l'Apple TV, Wi-Fi fourni par l'iPhone) ;
- le dernier état de la lampe (allumage, luminosité, couleur).

## Décision
- **`SeqMapKvBlobStore`** de `rs-matter-embassy` comme stockage clé-valeur. Il s'appuie sur `sequential-storage`, qui écrit en ajoutant à la suite et répartit l'usure sur les pages de la flash.
- Zone utilisée : la **partition NVS** de la table des partitions (24 Ko à 0x9000 avec la table par défaut d'`espflash`). Son adresse est **lue dans la table** au démarrage (`esp-bootloader-esp-idf`), pas écrite en dur. C'est la forme de partition pour laquelle `rs-matter-embassy` teste ce stockage.
- Accès à la flash par `esp-storage` (bloquant), adapté en asynchrone par `embassy-embedded-hal` (`BlockingAsync`), comme l'attend le stockage.
- **Le dernier état de la lampe n'a pas de code à nous** : les clusters On/Off, Level Control et Color Control de `rs-matter` se sauvegardent eux-mêmes, 3 s après le dernier changement, et se rechargent au démarrage. La bande reçoit alors l'état restauré par le chemin habituel (`LIGHT_CHANGED`).

## Alternatives envisagées
- **Partition dédiée** (table de partitions personnalisée) : plus de place, mais un fichier de plus à maintenir et à passer à `espflash`. 24 Ko suffisent a priori pour une seule fabric (l'Apple TV). À reconsidérer si la place manque.
- **Sauvegarder nous-mêmes le `LightState`** : inutile, les clusters le font déjà et respectent les règles de Matter (attribut `StartUpOnOff`, etc.).
- **Format NVS d'ESP-IDF** : pas de lecteur `no_std` en Rust, et rien à partager avec ESP-IDF.

## Conséquences
- La lampe reste appairée après un redémarrage ou un nouveau `cargo run`. `espflash` n'efface pas la partition NVS en flashant le programme.
- Pour **repartir de zéro** (réappairer depuis le début), il faut effacer la partition NVS à la main (commande dans `docs/mise-en-route.md`). Un bouton de remise à zéro viendra avec l'étape 4d.
- Après une coupure de courant, la lampe revient à son dernier état, mais seulement après le test des canaux (~6 s). Ce test pourra être retiré une fois la bande validée.
- Un changement perdu si le courant est coupé moins de 3 s après (délai d'écriture des clusters, qui limite l'usure de la flash).
- Vérifié sur la carte (2026-11-25) : l'appairage survit au redémarrage et au reflash ; ~3 Ko de données dans les 24 Ko (deux *fabrics* d'environ 1,2 Ko chacune, l'abonnement, les réglages).
