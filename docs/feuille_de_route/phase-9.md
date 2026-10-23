# Phase 9 — Contenu et équilibrage

- Nouvelles classes et sorts, nouvelles arènes, monde à sa taille définitive.
- Équilibrage par simulations de masse, avec rechargement à chaud des fichiers RON.
- Recalibrage du modèle statistique et réentraînement des modèles ML après chaque changement important.
- Premier vrai entraînement des modèles par classe (repoussé de la phase 7), une fois les classes, leurs sorts et les règles du combat achevés : `python/train.py <classe> duel` puis `melee` pour chaque classe, puis `python/export.py --version N`. Ensuite, réentraîner une classe quand elle change : obligatoire si la forme de l'observation ou des actions change (nouvelle classe, plus de sorts que la classe qui en a le plus, nouvelle valeur observée ; le test `every_class_has_a_recruit_and_a_veteran_model_fitting_the_environment` échoue alors), conseillé si seuls des chiffres changent (dégâts, recharges, stats : le modèle joue encore, mais moins bien). À ce moment, faire jouer chaque adversaire par le modèle de sa propre classe pendant l'entraînement, et vérifier le jalon de la phase 7 (10 000 duels contre l'IA scriptée).
- Finitions : sons, effets visuels, thème egui, lisibilité sur Steam Deck.
- Graphismes et sons définitifs, aux formats de [Formats_graphiques.md](../Formats_graphiques.md), sous les mêmes noms que ce qu'ils remplacent. Qui les produit reste à décider (voir « Points encore ouverts » du README). Tout est provisoire aujourd'hui :
  - combattants : calques en gris générés par un script (`assets/fighters/`, carrure `average`, cheveux `short` et `long`) ; manquent bouclier et deuxième arme, et les personnages uniques ;
  - arènes : sol, piliers, gradins, mines et taches de sang dessinés en formes et couleurs unies dans `fight_view.rs` ; aucune image ;
  - sons : `assets/placeholder/hit.wav`, `death.wav`, `music.wav` (WAV, alors que la musique doit être en OGG) ; aucun son d'interface, de sort ou de foule ;
  - interface : pas d'icônes, police par défaut d'egui.
- Une fois les vrais fichiers en place : supprimer `assets/placeholder/`, et tenir un fichier de crédits avec l'auteur et la licence de chaque fichier repris d'ailleurs (obligatoire pour les licences CC BY).

**Jalon :** taux de victoire de chaque classe entre 45 et 55 %, et écart réduit entre modèle statistique et vrais combats.
