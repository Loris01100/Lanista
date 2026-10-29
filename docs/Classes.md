# Classes

Classes qui ont remplacé les prototypes de `data/classes.ron` (mirmillon, augure), maintenant supprimés. Aucune statistique n'est fixée pour l'instant : ce document ne décrit que l'intention. Une classe a 3 sorts pour le moment.

## Règles communes

### Configurations

Chaque classe a deux configurations d'équipement ; le deuxième sort (ou l'effet d'un sort) en dépend. Le joueur choisit la configuration de chacun de ses combattants, et peut en changer chaque semaine, sans coût.

Chaque combattant a, pour chaque configuration de sa classe, une aisance, visible du joueur : s'il n'est pas à l'aise avec la configuration choisie, il perd des statistiques. L'aisance progresse avec une configuration souvent utilisée. Une recrue a une aisance tirée au hasard pour chacune : bonne dans les deux, ou très bonne dans l'une et très mauvaise dans l'autre, ou entre les deux.

### Bouclier

Le bouclier donne un bonus de défense. Il ne pare pas : seule une compétence qui le dit explicitement permet de parer.

### Double arme et ambidextrie

Un combattant est droitier, gaucher ou ambidextre (deux mains dominantes). Un combattant qui n'est pas ambidextre et se bat avec deux armes a un malus de dégâts : l'arme de sa main faible ne frappe pas à 100 %. Blessé, il perd des statistiques.

### Équipement refusé

Le combattant peut refuser son armure, jamais ses armes.

### Attaque automatique

Chaque type d'arme a une attaque automatique, hors des 3 sorts, au coût faible (selon l'arme). Le combattant la porte en dernier recours, quand aucun de ses sorts n'est à la fois payable et utile sur le moment (plus assez de mana ou d'endurance, mine sur une cible trop proche, rien à ramasser, tir bloqué par un pilier) : il peut ainsi toujours attaquer un peu. Les types d'armes et leurs attaques seront fixés à l'équilibrage.

### Saignement

Certains coups ont un pourcentage de chance de faire saigner la partie du corps touchée : la cible perd alors des PV pendant une durée fixe, propre à chaque partie du corps. Les saignements s'additionnent d'une partie du corps à l'autre (tête et bras : deux saignements), pas sur une même partie.

Un saignement peut tuer, ou pousser à l'abandon. Il laisse au plus une blessure moyenne après le combat.

## Gladiateur

Combattant de corps à corps uniquement, armes tranchantes uniquement.

| Configuration | Équipement |
| --- | --- |
| Épée et bouclier | une épée, un bouclier |
| Double épée | deux épées |

| Emplacement | Sort | Description |
| --- | --- | --- |
| 1 | Slash sanguin | Coup tranchant aux dégâts immédiats faibles, avec une chance de faire saigner. |
| 2 | Bouclier tranchant (épée et bouclier) | Coup porté avec le bouclier, tranchant. |
| 2 | Lacération (double épée) | Enchaînement des deux épées, aux dégâts immédiats faibles, avec une chance de faire saigner. Les deux coups touchent ensemble ou ratent ensemble, avec les mêmes dégâts : jamais une lacération d'un seul coup. |
| 3 | Guillotine | Coup peu précis aux dégâts élevés. Le premier coup sur une cible ne fait que de lourds dégâts (avec la chance de tranchage habituelle) ; à partir du deuxième, s'il touche une partie du corps qui peut être tranchée (tête, bras, jambe), il la tranche ; sur le torse, seulement des dégâts. |

## Maître du terrain

Combattant qui tire parti du sol de l'arène. Les dagues runiques frappent au corps à corps (faible tranchant) et servent à lancer des sorts. Il ne frappe avec ses dagues que par l'attaque automatique, quand aucun de ses sorts n'est payable et utile.

| Configuration | Équipement |
| --- | --- |
| Dague et bouclier | une dague runique, un bouclier |
| Double dague | deux dagues runiques |

| Emplacement | Sort | Coût | Description |
| --- | --- | --- | --- |
| 1 | Récolte | endurance | Ramasse l'élément du sol de l'arène, celui de son continent. |
| 2 | Jette | endurance et une partie des éléments ramassés | Lance à distance les éléments ramassés sur un adversaire. |
| 3 | Mine | mana | Lance une mine (dague et bouclier) ou deux mines (double dague). |

### Éléments

L'élément ramassé dépend du continent du colisée, quel que soit le type d'arène. La quantité portée au plus sera fixée aux essais.

| Continent | Élément | Effet jeté |
| --- | --- | --- |
| Europe | Pierre | dégâts contondants (peut étourdir) |
| Afrique | Sable | dégâts faibles, et une courte baisse de précision, qui s'additionne si l'on en reçoit plusieurs |
| Amériques | Obsidienne | dégâts tranchants, avec une chance de faire saigner |
| Océanie | Varech urticant | dégâts faibles ; retient brièvement la cible, puis le poison lui retire des PV quelques secondes |
| Asie | Éclats de jade | dégâts comme si la cible n'avait pas de bouclier : il ignore le bonus de défense du bouclier, rien de plus contre un adversaire sans bouclier |

### Mines

Une mine est lancée au sol ; elle s'active après un délai et reste 30 secondes. Tout le monde la voit. Elle explose sous quiconque marche dessus, allié, adversaire ou le poseur lui-même, et blesse tous ceux qui sont dans la zone de l'explosion ; l'explosion peut étourdir et trancher. Le nombre de mines au sol en même temps sera fixé à l'équilibrage. La deuxième mine de la double dague tombe plus loin de la première que le rayon de son explosion.

## Chasseur

Archer qui combat à distance. Ses deux configurations ont les mêmes sorts : l'arc change la puissance et la recharge de tous ses tirs, attaque de base comprise, et sa vitesse.

| Configuration | Équipement | Effet |
| --- | --- | --- |
| Arc lourd | un arc à deux mains | tirs à 130 % de puissance et 140 % de recharge, vitesse réduite |
| Arc léger | un arc à deux mains | moitié moins de puissance et recharge deux fois plus rapide que l'arc lourd (65 %, 70 %), vitesse accrue : deux tirs valent un tir lourd |

| Emplacement | Sort | Description |
| --- | --- | --- |
| 1 | Tir précis | Tir qui ne rate jamais, même aveuglé. |
| 2 | Pluie de flèches | Tir peu précis qui passe au-dessus des piliers et couvre une zone : chacun de ceux qui s'y trouvent, hors le chasseur et son allié, risque une flèche. |
| 3 | Piège à ours | Piège posé aux pieds de la cible, que seul le chasseur voit. Armé, il mord le premier qui marche dessus et le retient : il ne bouge ni n'agit plus, mais reste frappable et prend plus de dégâts. Le premier coup qui lui ôte des PV le libère ; sinon, il se libère au bout d'un temps fixe. |
| Attaque de base | Tir simple | Flèche simple, peu précise, aux dégâts modérés. |

## Ce qui reste à faire

Les étapes déjà faites (partie du corps touchée, modificateurs par sort, effets dans la durée, attaque automatique, configurations, aisance, armure refusée seule, éléments du sol, mines, les trois classes, retrait du mirmillon et de l'augure) sont décrites dans `docs/feuille_de_route/phase-5-bis.md`. Chaque étape finit par `cargo test`, clippy et, quand le combat change, `calibrate` puis `compare`.

- **IA apprise** (`learning.rs`, `learned.rs`, `lanista_py`, `python/`) : l'observation gagne la configuration, l'aisance, les saignements, la réserve d'éléments et les mines proches ; les actions gagnent l'attaque automatique ; réexporter les substituts (`export.py`).
- **Modèle statistique** (`stat_model.rs`) : il gagne la configuration et l'aisance ; relancer `calibrate` et `compare`.
- **Documents** : reporter dans la feuille de route (phase 9) et le plan de conception ce qui a été fait.

## Questions ouvertes

- Les types d'armes de l'attaque automatique, à l'équilibrage.
