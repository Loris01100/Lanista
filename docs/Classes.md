# Classes

Classes prévues pour remplacer les prototypes actuels de `data/classes.ron` (mirmillon, augure), qui seront supprimés. Aucune statistique n'est fixée pour l'instant : ce document ne décrit que l'intention. Une classe a 3 sorts pour le moment.

## Règles communes

### Configurations

Chaque classe a deux configurations d'équipement ; le deuxième sort (ou l'effet d'un sort) en dépend. Le joueur choisit la configuration de chacun de ses combattants.

Chaque combattant a, pour chaque configuration de sa classe, une aisance, visible du joueur : s'il n'est pas à l'aise avec la configuration choisie, il perd des statistiques. L'aisance progresse avec une configuration souvent utilisée.

### Attaque automatique

Chaque type d'arme a une attaque automatique, hors des 3 sorts, au coût faible (selon l'arme). Le combattant la porte en dernier recours, quand il n'a plus assez de mana ou d'endurance pour ses sorts : il peut ainsi toujours attaquer un peu.

### Saignement

Certains coups ont un pourcentage de chance de faire saigner : la cible perd alors des PV pendant un moment.

## Gladiateur

Combattant de corps à corps uniquement, armes tranchantes uniquement.

| Configuration | Équipement |
| --- | --- |
| Épée et bouclier | une épée, un bouclier |
| Double épée | deux épées (ambidextre) |

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
| 1 | Récolte | endurance | Ramasse des éléments du sol de l'arène : sable, pierre, plus tard bois. |
| 2 | Jette | endurance et une partie des éléments ramassés | Lance les éléments ramassés sur un adversaire : des dégâts, plus ou moins selon l'élément ; le sable baisse en plus sa précision. |
| 3 | Mine | mana | Pose une mine (dague et bouclier) ou deux mines (double dague). |

### Mines

Une mine reste 30 secondes sur le sol. Elle explose sous quiconque marche dessus, allié, adversaire ou le poseur lui-même, et blesse tous ceux qui sont dans la zone de l'explosion.

## Questions ouvertes

- Les éléments qu'offre chaque arène attendent la définition des arènes ; le bois viendra plus tard.
- Les effets des éléments autres que le sable (pierre, bois) : des dégâts seulement, pour l'instant.
