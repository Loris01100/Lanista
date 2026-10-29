# Propositions — ce que j'ajouterais et ce que je modifierais

Version du 29 octobre 2026. Liste de propositions, pas une décision : rien n'est fait dans le code. Chaque idée dit pourquoi, où elle se brancherait, et son risque. Les idées déjà écartées par le plan de conception (sponsor, prêt, échange, exhibition, défis reçus, loyauté) ne sont pas reprises.

Priorité : **P1** à faire avant la sortie, **P2** gros gain pour peu d'effort, **P3** si le temps le permet.

## 1. Corrections de documents

Faites le 29 octobre 2026 : le plan de conception (tableau des blessures, armure seule refusée, coups contondants, renommée, liste des écrans), la feuille de route (modules réels de `lanista_core`, `notify` et `requirements.txt` retirés) et `docs/Classes.md` (l'historique renvoie à `phase-5-bis.md`, ne reste que ce qui est à faire).

Reste : AGENTS.md dit le code « proprietary », alors que la feuille de route annonce un code ouvert. À trancher avec la licence (voir 6.1).

## 2. Combat

### 2.1 Deux classes de plus (P1)

Trois classes, c'est peu pour un jeu dont la variété vient des combattants, et aucune n'est contondante depuis le retrait de l'augure : l'étourdissement ne vient plus que de la pierre d'Europe et des mines. Je proposerais deux classes qui remplissent des trous, pas des copies :

- **Rétiaire** (filet et trident, distance moyenne). Comble le vide entre le corps à corps et l'arc.
  1. *Filet* : lancé à courte portée, retient la cible comme le piège à ours (réutiliser l'effet), mais se voit et s'esquive.
  2. *Coup de trident* : allonge supérieure à l'épée ; configuration « trident et filet » ou « trident et dague ».
  3. *Ramener* : tire une cible retenue vers soi.
  Un nouveau type de dégâts **perforant** (`DamageKind::Piercing`) ignorerait une part de l'armure : il donne enfin un sens au refus de l'armure face à certaines classes.
- **Pugiliste** (cestes, contondant pur). La classe de l'étourdissement : faibles dégâts, beaucoup d'étourdissements, des K.-O. plutôt que des morts. Donne aux clubs prudents un style qui tue peu, en accord avec le tempérament survivant.

Données dans `data/classes.ron`, textes dans `fr.ftl`, sprites aux formats de `Formats_graphiques.md`. À chaque classe : `batch`, `calibrate`, `compare`, et l'IA apprise à réentraîner (phase 9).

### 2.2 La grâce du public, *missio* (P2)

Le moment le plus célèbre de l'arène n'existe pas dans le jeu : quand un combattant est K.-O. ou à terre après un abandon raté, l'éditeur des jeux demande au public, qui décide de la grâce ou de la mort.

- La décision dépend de la renommée du vaincu, du spectacle qu'il a donné (dégâts infligés, durée), du prestige du colisée et de l'ego du vainqueur.
- Un combattant gracié gagne de la renommée ; un vaincu sans éclat dans un grand colisée risque sa vie.
- Affichage : une pause courte dans le combat, une jauge de la foule, puis le pouce.

Risque : la mortalité, réglée avec soin en phase 5 bis, remonterait. Garder la mort par *missio* rare et la mesurer avec `temperaments` ; le modèle statistique doit apprendre ce tirage (`StatModel`) pour que les rivaux le subissent aussi.

### 2.3 De nouvelles arènes (P2)

Les trois types (sable, piliers, étroite) changent la tactique ; quelques autres renforceraient l'identité des continents :

| Arène | Effet | Pour qui |
| --- | --- | --- |
| Fosses | Trous au sol qu'on contourne ; un combattant poussé ou en fuite peut y tomber (étourdi) | Colisée mineur d'Asie |
| Eau peu profonde | Tous ralentis, sauf le rétiaire ; les mines s'arment plus lentement | Océanie |
| Nuit aux torches | Portée des tirs réduite hors des zones éclairées | Un combat spécial par saison |

Une arène reste une ligne de `arenas` dans `data/combat.ron` ; seul le nouveau comportement (chute, lumière) demande du code.

### 2.4 L'écran de combat (P2)

- Vitesse x2 et x4, en plus de x0,5, x1, x1,5 (`SPEEDS` dans `combat_screen.rs`) : une mêlée de trois minutes est longue à x1,5.
- Pause et avance d'une décision à la fois, pour comprendre ce que fait l'IA (utile aussi pour revoir les combats pendant l'entraînement).
- Un ralenti automatique d'une seconde sur un membre tranché, une mort ou un K.-O.
- Des infobulles sur les effets en cours d'un combattant (saignement, retenu, précision baissée), qui se lisent aujourd'hui dans le journal seulement.

## 3. Management

### 3.1 Moins de clics chaque semaine (P1)

Un jeu de semaines répétées doit se jouer vite. Aujourd'hui chaque combattant se règle à la main.

- « Même plan que la semaine dernière » par combattant, et un bouton pour tout le club.
- Des alertes bloquantes avant « semaine suivante » : un champion dont la défense tombe cette semaine, un contrat qui finit, un combattant blessé inscrit, une maladie sévère inscrite.
- Raccourcis clavier : semaine suivante, onglets du club, vitesse du combat (et donc boutons de la manette plus tard, phase 10).

### 3.2 Préparer un combat contre un adversaire (P2)

Les graines permettent déjà de revoir un combat (« Revoir le combat »). On pourrait ouvrir les combats passés **d'un adversaire** visibles du joueur : ses trois derniers duels contre des combattants du joueur, et ce que le recruteur en dit (« ouvre toujours par Guillotine », « abandonne tôt »). Le recruteur prend un rôle en dehors du marché, ce qui donne une raison à un club riche de le garder.

Attention à ne pas donner l'exhibition par la bande : seulement des combats déjà livrés.

### 3.3 Fin de contrat négociée (P3)

Les contrats existent (`economy::Contract`). À l'intersaison, un combattant qui a beaucoup gagné en renommée pourrait demander des gages plus hauts, d'autant plus fort que son ego est grand ; le joueur accepte, négocie ou le laisse partir au marché. Le dernier pilier (« chaque décision a un coût ») s'appliquerait alors aussi aux stars.

### 3.4 Entraînement en duel entre combattants du club (P3)

Une option d'entraînement de plus : deux combattants du club s'affrontent à l'entraînement, armes émoussées (pas de tranchage, pas de mort). Gain d'aisance avec la configuration et un peu de progression, contre de la forme et un petit risque de blessure légère. Donne un usage à la configuration qu'on veut apprendre sans la risquer en colisée.

## 4. Monde et mémoire

### 4.1 La nécropole et le panthéon (P2)

Le jeu fabrique des histoires (rancunes, membres perdus, titres) mais les oublie : le fil de nouvelles ne garde que ses dernières semaines. Deux pages de plus, depuis l'Acta Diurna ou l'encyclopédie :

- **Nécropole** : les combattants morts du club du joueur, avec qui les a tués, où, leur bilan et leurs titres. Une épitaphe tirée selon leur tempérament.
- **Panthéon** : les plus grands combattants du monde (titres, victoires, saisons de règne), à partir des palmarès déjà gardés (`Colosseum::reigns`).

Peu de données en plus : une entrée courte par mort du club, le reste se lit dans ce qui existe.

### 4.2 Relations entre clubs de l'ordinateur (P3)

Annoncé « plus tard, peut-être » en phase 6 bis. Ce serait la suite naturelle : deux clubs de l'ordinateur qui se haïssent s'achètent moins, se chassent en mêlée, et l'Acta Diurna en parle. Le monde vivrait sans le joueur au centre. Gros chantier : le faire après la phase 8.

## 5. Grands événements (phase 8)

Rien n'est fait. Quelques propositions pour qu'ils soient des moments, pas seulement des semaines de plus :

- **Championnat du monde** : un tableau visible des poules et de l'élimination directe, mis à jour chaque semaine, avec les combattants du joueur surlignés.
- **Inter-Coliseum** : le 5 contre 5 demande des rôles que les classes actuelles n'ont pas (aucune ne protège ni ne soigne). Soit on accepte un 5 contre 5 brutal, soit une classe de soutien vient avec la phase 9. Je trancherais avant de l'écrire, parce que l'IA apprise doit s'entraîner en équipe.
- **Succès** : les choisir pour qu'ils poussent à essayer les mécaniques plutôt qu'à répéter. Exemples :
  - gagner un titre avec un combattant manchot ;
  - remporter une mêlée à 20 avec deux combattants du même club encore debout à la fin ;
  - mener un combattant pauvre recruté gratuitement jusqu'au titre de Rome ;
  - faire d'un club rival déclaré un allié ;
  - un combattant qui obtient ses six traits ;
  - remonter d'un club pauvre à riche en trois saisons.
- **Tutoriel** : l'ordre de déblocage du plan de conception est bon ; ajouter les configurations et l'aisance (après le repos et l'entraînement), puis les relations entre clubs (après les défis).

## 6. Technique

### 6.1 Licence (P1)

Point ouvert depuis longtemps, et il bloque la page Steam, le README public et `deny.toml`. Ma recommandation : code sous MIT/Apache-2.0 (rien à vérifier côté dépendances), graphismes et sons tous droits réservés ou CC BY-NC.

### 6.2 Équilibrage plus rapide (P2)

- Rechargement à chaud des fichiers RON (`notify`, retiré de la feuille de route), seulement dans `lanista` en debug : changer une recharge dans `classes.ron` et revoir le combat sans relancer.
- Un mode « rejouer cette graine » dans l'écran de combat, qui relit `data/` avant de rejouer : la même graine montre l'effet du changement.
- `requirements.txt` (ou `pyproject.toml`) dans `python/`, figé, pour que l'environnement d'entraînement se recrée à l'identique en phase 9.

### 6.3 Sauvegardes (P2)

Fait : rotation de trois sauvegardes automatiques et test de taille sur dix saisons (voir la phase 4).

### 6.4 Accessibilité et Steam Deck (P1, avant la phase 10)

- La jauge rouge de la mêlée et les cœurs rouges des mines ne doivent pas reposer sur la seule couleur : ajouter une forme ou un contour (daltonisme).
- Taille du texte réglable dans les options : à 1280×800, les tableaux du marché et des clubs seront serrés.
- Tester tôt la navigation egui au clavier seul, qui annonce ce que donnera la manette.

## 7. Ce que je ne ferais pas

- Plus de 3 combattants actifs par club tant que la vue multi-combats et les tours de semaine ne sont pas mesurés : chaque combattant de plus multiplie les clics.
- Des installations du ludus à acheter : le personnel couvre déjà ce rôle, ce serait un doublon.
- Une économie de l'équipement : elle casserait le pilier de l'égalité matérielle.
