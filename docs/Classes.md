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
| 3 | Guillotine | Coup peu précis aux dégâts élevés. Le premier coup sur une cible ne fait que de lourds dégâts (avec la chance de tranchage habituelle) ; à partir du deuxième, s'il touche une partie du corps qui peut être tranchée (tête, bras, jambe), il la tranche ; sur le torse, seulement des dégâts. |

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

1. **Partie du corps touchée** (`combat.rs`, `fighter/injury.rs`) — fait : chaque coup qui touche tire la partie du corps qu'il touche, avec les poids `hits` de `data/combat.ron`. Le tranchage (`combat::severs`) ne coupe que cette partie, jamais le corps. Le journal de combat la dit.
2. **Modificateurs par sort** (`design_data::SpellData`, `combat.rs`) — fait : deux champs facultatifs dans `data/classes.ron`. `accuracy_percent` (100 par défaut) garde ce pourcentage de la chance de toucher (`combat::spell_hit_percent`, aussi lu par l'IA utilitaire), même dans une arène qui touche toujours. `sure_cut_hits` : à partir de ce nombre de coups portés par le sort sur une même cible (`Fighter::cleaving_hits`), un coup tranchant tranche à coup sûr la tête ou le membre touché (`combat::severs`), jamais le corps ni un moignon ; Guillotine aura `Some(2)`.
3. **Effets dans la durée** (`fighter.rs`, `combat.rs`, `log.rs`) — fait : des effets qui durent sur le combattant (`Fighter::effects`), chacun avec ses ticks restants, décomptés par `combat::wear_off`. Deux pour commencer :
   - le saignement, un par partie du corps : sa chance par sort (`bleed_percent` dans `data/classes.ron`), la part des PV max perdue chaque seconde et la durée par partie dans `bleed` de `data/combat.ron`. Ses dégâts comptent pour celui qui l'a causé (aussi pour les contrôles de la mêlée). Il peut tuer (en passant par la survie au coup fatal) ou pousser à l'abandon ; sinon, il laisse une blessure légère ou moyenne là où le combattant a saigné en premier, à la place du tirage habituel (`injury::bleeding_wound`) ;
   - la baisse de précision (`accuracy_drop` d'un sort : un pourcentage et une durée), qui s'additionne, pour le sable de Jette.
   Le combat montre une petite flaque sous un combattant qui saigne (`fight_view.rs`). Le modèle statistique ne tire pas encore de saignement, et l'IA utilitaire ne compte pas encore le saignement dans la valeur d'un sort.
4. **Attaque automatique** (`design_data.rs`, `fighter.rs`) — fait : une attaque par type d'arme (`weapons` de `data/combat.ron`), hors des emplacements de sort, au coût faible. La classe nomme son arme (`ClassData::weapon`). L'attaque s'ajoute en dernier aux sorts du combattant (`Fighter::auto_attack`). Elle n'est lançable que si aucun autre sort n'est payable (`Fighter::can_cast`) ; l'IA utilitaire la joue alors comme un sort. L'arme vient maintenant de la configuration (étape 5).
5. **Configurations et équipement** (`data/classes.ron`, `design_data::LoadoutData`, `fighter/equipment.rs`, `fighter/career.rs`) — fait :
   - les `spells` d'une classe sont ceux des emplacements 1 et 3 ; chacune de ses deux configurations (`loadouts`) apporte le sort du deuxième emplacement, son arme (donc son attaque automatique) et sa main secondaire (`OffHand` : libre, bouclier ou deuxième arme). Configurations d'essai : glaive et bouclier (Charge au bouclier) ou double glaive (Double taille, tranchante) pour le mirmillon, bâton (Trait de feu) ou bâton et bouclier (Coup de bouclier) pour l'augure ;
   - le bouclier relève les deux défenses (`shield_defense_percent`, 115 %), la deuxième arme baisse l'attaque au corps à corps d'un combattant qui n'est pas ambidextre (`off_hand_attack_percent`, 85 %), dans `equipment` de `data/fighter.ron` (`OffHand::stats`) ;
   - l'ambidextrie (`CareerFighter::ambidextrous`, `ambidextrous_percent`, 5 %) est tirée du même jet que la main dominante, parmi les droitiers : les tirages ne changent pas. Les blessures d'un ambidextre suivent sa main droite ;
   - chaque combattant garde sa configuration (`CareerFighter::loadout`, sauvegardée). Le plan de la semaine la porte (`WeekPlan::loadout`) : le joueur la change sans coût dans le plan (`club_screen`), et la fiche la montre avec l'ambidextrie. Un combattant créé prend, comme les clubs rivaux chaque semaine (`rivals::plan`), celle que donne `CareerFighter::preferred_loadout`, celle où il est le plus à l'aise (étape 6) ;
   - `CareerFighter::combat_fighter` en tire les sorts (`Fighter::in_loadout`) et les modificateurs ; `Fighter::new` prend la première configuration ;
   - ni le modèle statistique ni l'IA apprise ne voient la configuration (étape 11). Les sprites n'ont ni bouclier ni deuxième arme (étape 10).
6. **Aisance** (`fighter/equipment.rs`, `fighter/career.rs`, `market.rs`) — fait :
   - une aisance de 0 à 100 par configuration (`CareerFighter::ease`), réglée par `ease` dans `data/fighter.ron` (`EaseRules`) ;
   - à la création, une configuration forte et un déséquilibre sont tirés (`EaseRules::random`) : de 70 dans les deux jusqu'à 100 dans l'une et 10 dans l'autre ;
   - sous 70 (`comfortable`), l'attaque, les défenses et la précision baissent, jusqu'à 20 % à l'aisance 0 (`EaseRules::stats`) ;
   - chaque combat livré dans une configuration lui ajoute 4 points (`per_fight`, dans `CareerFighter::after_fight`), jusqu'à 100 ;
   - la fiche montre l'aisance avec chaque configuration ; pour un combattant extérieur, elle se voit une fois observé, avec un recruteur de niveau 3 ou plus (`Scouting::ease`, `market::ease_seen`) ;
   - un combattant créé et les combattants des rivaux prennent la configuration où ils sont le plus à l'aise (`CareerFighter::preferred_loadout`) ;
   - l'ambidextrie ne pèse pas sur le tirage : elle retire seulement le malus de la deuxième arme.
7. **Équipement refusé : l'armure seule** (`fighter/equipment.rs`, `fighter/fame.rs`, `data/fighter.ron`, `club_screen`, `fr.ftl`) — fait : `Refused` ne garde que `armour` ; le malus d'attaque (`weaponless_attack_percent`), les coups à mains nues, les bonus de célébrité `refused_weapon` et `refused_all` et la case « Sans arme » sont retirés. L'observation de l'IA apprise garde la place de l'arme, toujours à 0, jusqu'à l'étape 11.
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
