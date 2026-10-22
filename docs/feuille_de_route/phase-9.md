# Phase 9 — Contenu et équilibrage

- Nouvelles classes et sorts, nouvelles arènes, monde à sa taille définitive.
- Équilibrage par simulations de masse, avec rechargement à chaud des fichiers RON.
- Recalibrage du modèle statistique et réentraînement des modèles ML après chaque changement important.
- Premier vrai entraînement des modèles par classe (repoussé de la phase 7), une fois les classes, leurs sorts et les règles du combat achevés : `python/train.py <classe> duel` puis `melee` pour chaque classe, puis `python/export.py --version N`. Ensuite, réentraîner une classe quand elle change : obligatoire si la forme de l'observation ou des actions change (nouvelle classe, plus de sorts que la classe qui en a le plus, nouvelle valeur observée ; le test `every_class_has_a_recruit_and_a_veteran_model_fitting_the_environment` échoue alors), conseillé si seuls des chiffres changent (dégâts, recharges, stats : le modèle joue encore, mais moins bien). À ce moment, faire jouer chaque adversaire par le modèle de sa propre classe pendant l'entraînement, et vérifier le jalon de la phase 7 (10 000 duels contre l'IA scriptée).
- Finitions : sons, effets visuels, thème egui, lisibilité sur Steam Deck.

**Jalon :** taux de victoire de chaque classe entre 45 et 55 %, et écart réduit entre modèle statistique et vrais combats.
