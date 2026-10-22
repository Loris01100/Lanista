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

## Plan de mise en œuvre

Chaque étape laisse le jeu jouable avec mirmillon et augure, qui servent de terrain d'essai, et finit par `cargo test`, clippy et, quand le combat change, `calibrate` puis `compare`. L'IA utilitaire suit à chaque étape ; l'IA apprise ne suit qu'à la fin (étape 11), ses modèles n'étant que des substituts jusqu'à la phase 9.

1. **Partie du corps touchée** (`combat.rs`, `fighter/injury.rs`) : chaque coup tire la partie du corps qu'il touche, avec des poids dans `data/combat.ron`. Le tranchage actuel (`combat::severed_part`) part de la partie touchée au lieu de la tirer après coup. Le journal de combat la dit.
2. **Modificateurs par sort** (`design_data::SpellData`, `combat.rs`) : un modificateur de précision (Guillotine peu précise) et un tranchage forcé de la partie touchée (Guillotine), facultatifs dans `data/classes.ron`.
3. **Effets dans la durée** (`fighter.rs`, `combat.rs`, `log.rs`) : des effets qui durent sur le combattant, chacun avec sa durée et ses ticks. Deux pour commencer :
   - le saignement, un par partie du corps, sa chance par sort et sa durée par partie dans les données ; il peut tuer (en passant par la survie au coup fatal) ou pousser à l'abandon, et laisse au plus une blessure moyenne (`injury`) ;
   - la baisse de précision, qui s'additionne, pour le sable de Jette.
   Le combat montre un combattant qui saigne (`fight_view.rs`).
4. **Attaque automatique** (`design_data.rs`, `ai.rs`) : une attaque par type d'arme, hors des emplacements de sort, au coût faible, que l'IA utilitaire porte quand aucun sort n'est possible.
5. **Configurations et équipement** (`data/classes.ron`, `design_data::ClassData`, `fighter/equipment.rs`, `fighter/career.rs`) :
   - chaque classe décrit ses deux configurations : l'équipement (arme(s), bouclier) et le sort du deuxième emplacement ;
   - le bouclier donne son bonus de défense, la double arme le malus de la main faible ;
   - la main dominante gagne l'ambidextrie, avec sa chance à la création ;
   - chaque combattant garde sa configuration (sauvegardée), choisie par le joueur chaque semaine (`WeekPlan`, `club_screen`), par les clubs rivaux et les agents libres (`rivals.rs`, `state`) ;
   - `CareerFighter::combat_fighter` en tire les sorts et les modificateurs.
6. **Aisance** (`fighter/career.rs`, `fighter/progression.rs`, `market.rs`) : une aisance par configuration, tirée à la création (bonne partout, ou forte dans l'une et faible dans l'autre), des malus de stats quand elle est faible, une progression avec les combats livrés dans la configuration ; visible sur la fiche, et pour un combattant extérieur selon ce que voit le recruteur ; les rivaux choisissent la configuration où le combattant est le plus à l'aise.
7. **Équipement refusé : l'armure seule** (`fighter/equipment.rs`, `fighter/fame.rs`, `data/fighter.ron`, `club_screen`, `fr.ftl`) : retirer `Refused::weapon`, son malus, les bonus de célébrité `refused_weapon` et `refused_all`, et leur choix à l'écran.
8. **Éléments du sol** (`data/combat.ron`, `combat.rs`, `design_data::Cost`) :
   - chaque type d'arène dit son élément (sable, pierre) ;
   - le combattant porte une réserve d'éléments, avec un maximum ;
   - un sort peut coûter plusieurs choses (endurance et éléments) ;
   - Récolte : un sort sans cible ni dégâts, qui remplit la réserve ;
   - Jette : une puissance et un type de dégâts par élément (pierre contondante, sable avec sa baisse de précision de l'étape 3) ;
   - l'IA utilitaire sait quand ramasser et quand jeter.
9. **Mines** (`combat.rs`, `melee.rs`, `ai.rs`, `fight_view.rs`) :
   - des objets sur l'arène (position, poseur, délai d'activation, 30 secondes de vie), lancés à un point de chute ;
   - l'explosion au contact de quiconque, des dégâts de zone, une chance d'étourdir et de trancher pour chaque combattant touché ;
   - un nombre maximum de mines au sol ;
   - l'IA utilitaire les voit et les contourne, comme les piliers (`combat::path_direction`) ;
   - l'écran de combat les dessine (format dans `docs/Formats_graphiques.md`).
10. **Les deux classes** (`data/classes.ron`, `fr.ftl`, `assets/fighters/`) : gladiateur et maître du terrain, avec des valeurs provisoires ; leurs sprites (bouclier, deux armes) en substituts.
11. **Retrait de mirmillon et augure, IA apprise et modèle statistique** :
    - retirer les deux anciennes classes, leurs clés de `fr.ftl`, leurs modèles de `assets/ai/` ; mettre à jour le test qui compte les classes (`design_data.rs`) et les snapshots `insta` ;
    - l'IA apprise (`learning.rs`, `learned.rs`, `lanista_py`, `python/`) : l'observation gagne la configuration, l'aisance, les saignements, la réserve d'éléments et les mines proches ; les actions gagnent l'attaque automatique ; réexporter les substituts (`export.py`) ;
    - le modèle statistique (`stat_model.rs`) gagne la configuration et l'aisance ; relancer `calibrate` et `compare`.
12. **Documents** : reporter dans la feuille de route (phase 9) et le plan de conception ce qui a été fait.

## Questions ouvertes

- Les éléments qu'offre chaque arène attendent la définition des arènes ; le bois viendra plus tard.
- Les types d'armes de l'attaque automatique, à l'équilibrage.
