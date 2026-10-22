# Classes

Classes prévues pour remplacer les prototypes actuels de `data/classes.ron` (mirmillon, augure), qui seront supprimés une fois les nouvelles classes en place ; ils restent jusque-là. Aucune statistique n'est fixée pour l'instant : ce document ne décrit que l'intention. Une classe a 3 sorts pour le moment.

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

Chaque type d'arme a une attaque automatique, hors des 3 sorts, au coût faible (selon l'arme). Le combattant la porte en dernier recours, quand il n'a plus assez de mana ou d'endurance pour ses sorts : il peut ainsi toujours attaquer un peu. Les types d'armes et leurs attaques seront fixés à l'équilibrage.

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
| 2 | Lacération (double épée) | Enchaînement des deux épées, aux dégâts immédiats faibles, avec une chance de faire saigner. |
| 3 | Guillotine | Coup peu précis aux dégâts élevés ; s'il touche une partie du corps qui peut être tranchée (tête, bras, jambe), il la tranche ; sur le torse, seulement des dégâts. |

## Maître du terrain

Combattant qui tire parti du sol de l'arène. Les dagues runiques frappent au corps à corps (faible tranchant) et servent à lancer des sorts. Il ne frappe avec ses dagues que par l'attaque automatique, quand il n'a plus de mana ni d'endurance.

| Configuration | Équipement |
| --- | --- |
| Dague et bouclier | une dague runique, un bouclier |
| Double dague | deux dagues runiques |

| Emplacement | Sort | Coût | Description |
| --- | --- | --- | --- |
| 1 | Récolte | endurance | Ramasse l'élément du sol de l'arène : sable, pierre, plus tard bois. |
| 2 | Jette | endurance et une partie des éléments ramassés | Lance à distance les éléments ramassés sur un adversaire. |
| 3 | Mine | mana | Lance une mine (dague et bouclier) ou deux mines (double dague). |

### Éléments

L'élément ramassé dépend de l'arène. La quantité portée au plus sera fixée aux essais.

| Élément | Effet jeté |
| --- | --- |
| Sable | dégâts faibles, et une courte baisse de précision, qui s'additionne si l'on en reçoit plusieurs |
| Pierre | dégâts contondants (peut étourdir) |
| Bois | à venir |

### Mines

Une mine est lancée au sol ; elle s'active après un délai et reste 30 secondes. Tout le monde la voit. Elle explose sous quiconque marche dessus, allié, adversaire ou le poseur lui-même, et blesse tous ceux qui sont dans la zone de l'explosion ; l'explosion peut étourdir et trancher. Le nombre de mines au sol en même temps sera fixé à l'équilibrage.

## Questions ouvertes

- Les éléments qu'offre chaque arène attendent la définition des arènes ; le bois viendra plus tard.
- Les types d'armes de l'attaque automatique, à l'équilibrage.
