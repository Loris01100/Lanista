# Classes

Classes prévues pour remplacer les prototypes actuels de `data/classes.ron` (mirmillon, augure), qui seront supprimés. Aucune statistique n'est fixée pour l'instant : ce document ne décrit que l'intention. Une classe a 3 sorts pour le moment.

Chaque classe a deux configurations d'équipement ; le deuxième sort dépend de la configuration.

## Gladiateur

Combattant de corps à corps uniquement, armes tranchantes uniquement.

| Configuration | Équipement |
| --- | --- |
| Épée et bouclier | une épée, un bouclier |
| Double épée | deux épées (ambidextre) |

| Emplacement | Sort | Description |
| --- | --- | --- |
| 1 | Slash sanguin | Attaque tranchante de base. |
| 2 | Bouclier tranchant (épée et bouclier) | Coup porté avec le bouclier, tranchant. |
| 2 | Lacération (double épée) | Enchaînement des deux épées. |
| 3 | Guillotine | Coup puissant visant la tête. |

## Maître du terrain

Combattant qui tire parti du terrain de l'arène. Les dagues runiques frappent au corps à corps (faible tranchant) et servent à lancer des sorts.

| Configuration | Équipement |
| --- | --- |
| Dague et bouclier | une dague runique, un bouclier |
| Double dague | deux dagues runiques |

| Emplacement | Sort | Description |
| --- | --- | --- |
| 1 | Récolte | Ramasse des éléments du sol de l'arène (exemple : du sable). |
| 2 | Jette | Lance les éléments ramassés sur un adversaire (exemple : du sable dans les yeux). |
| 3 | Mine | Pose une mine (dague et bouclier) ou deux mines (double dague). |

L'attaque au corps à corps des dagues n'occupe pas d'emplacement de sort pour l'instant : à trancher (voir les questions ouvertes).

## Questions ouvertes

- Quels éléments ramasser sur chaque type d'arène (sable, piliers, étroite) et quel effet a chacun une fois jeté ?
- Le maître du terrain frappe-t-il avec ses dagues par un sort à part (ce qui ferait un quatrième sort) ou seulement via ses sorts ?
- Slash sanguin et Lacération font-ils saigner (des dégâts dans la durée) ou seulement des dégâts immédiats ?
- Guillotine : simple coup puissant, ou coup qui achève un adversaire affaibli et tranche plus souvent la tête ?
- La configuration est-elle choisie par le joueur, tirée au recrutement, ou modifiable d'une semaine à l'autre ?
- Ressources : le maître du terrain utilise-t-il du mana, de l'endurance, ou les éléments ramassés comme coût ?
