# Gladiator Manager — Plan de conception

Version du 27 septembre 2026

## Cadre du projet

Gladiator Manager est un projet passion et d'apprentissage, sans objectif commercial ni d'audience.

| Élément | Décision |
| --- | --- |
| Modèle | Gratuit, sans microtransactions |
| Plateforme | PC via Steam : Windows d'abord, puis Linux et Steam Deck |
| Web | Non prévu |
| Objectif de la publication | Apprendre tout le cycle, du prototype à la mise en ligne sur Steam |
| Coût à prévoir | 100 $ de Steam Direct (non remboursés pour un jeu gratuit) |

## Vision et piliers

Un jeu de management de gladiateurs en 2D, sous Godot (version .NET), où le joueur dirige un club sans jamais contrôler les combats. Il prépare, inscrit et soigne ses combattants, puis regarde en spectateur une IA les faire combattre. Le joueur ne peut pas perdre : il vise la complétion de tous les succès, et chaque combattant perdu l'oblige à remonter la pente.

Quatre piliers guident toutes les décisions :

1. **Chaque décision a un coût.** Combattre rapporte de l'argent et de la renommée mais coûte de la forme, augmente le risque de blessure, de maladie et de mort.
2. **L'égalité matérielle.** Les colisées fournissent l'équipement : la différence vient uniquement du combattant (stats, personnalité, état) et du manager.
3. **Un monde vivant.** D'autres clubs jouent en même temps, les classements bougent chaque semaine, les champions changent.
4. **Une IA de combat qui apprend.** C'est la signature du jeu, mais le jeu doit rester jouable avec une IA scriptée si le machine learning prend du retard.

## Boucle de jeu et calendrier

Le jeu avance semaine par semaine, et chaque combattant fait au maximum un combat par semaine. Toutes les décisions se prennent avant la simulation ; ensuite le joueur regarde et subit les conséquences.

```mermaid
flowchart LR
  A[Rapport de la semaine] --> B[Décisions par combattant<br/>repos, entraînement, soins]
  B --> C[Inscriptions<br/>colisée, défis envoyés ou reçus]
  C --> D[Simulation des combats<br/>de tout le monde]
  D --> E[Visionnage<br/>combats du joueur]
  E --> F[Comptes rendus<br/>+ sauvegarde auto]
  F --> G[Fin de semaine<br/>forme, progression, classements]
  G --> A
```

La fin de semaine applique tout ce qui ne dépend pas du joueur : récupération ou perte de forme, progression des stats, guérison, nouveaux risques de maladie, décisions des managers rivaux et mise à jour des classements.

### Structure d'une saison

Une saison correspond à une année de jeu, soit 30 semaines au total.

| Phase | Durée | Contenu |
| --- | --- | --- |
| Compétition | 22 semaines | La Classique dans chaque colisée, défis entre clubs |
| Grand événement | 4 semaines | Championnat du monde ou Inter-Coliseum selon l'année |
| Intersaison | 4 semaines | Vieillissement, retraites, arrivée des rookies, marché ouvert |

### Rythme des grands événements

Le championnat du monde a lieu aux saisons 4, 8, 12, etc. L'Inter-Coliseum a lieu aux saisons impaires (1, 3, 5, 7…). Ainsi il revient bien tous les deux ans sans jamais tomber la même année que le championnat du monde, et la règle du report devient inutile.

## Les combattants

Un combattant est défini par sept stats, un état physique (forme, blessures, maladie), une personnalité, un moral, une renommée et un âge. Tous ces éléments sont aussi des entrées de l'IA de combat, pour qu'un combattant blessé ou peureux se batte différemment.

### Stats

| Stat | Rôle |
| --- | --- |
| PV | Points de vie maximum |
| AC | Dégâts au corps à corps |
| AL | Dégâts à distance (armes et sorts) |
| PRÉ | Précision : chance de toucher, contre la VIT de la cible |
| VIT | Vitesse de déplacement, fréquence d'attaque et esquive |
| DC | Réduction des dégâts au corps à corps |
| DL | Réduction des dégâts à distance |

Formule de dégâts proposée (AC contre DC, AL contre DL) : la défense réduit sans jamais annuler, ce qui évite les combats infinis.

```latex
\text{Dégâts} = \text{Attaque} \times \frac{100}{100 + \text{Défense}}
```

Une attaque peut rater. La chance de toucher oppose la précision de l'attaquant à la vitesse de la cible, bornée pour qu'un coup ne soit jamais sûr ni impossible : à précision égale à la vitesse adverse, on touche à un taux de base (85 % pour l'instant), et jusqu'à deux fois ce taux avec une bien meilleure précision.

```latex
\text{Toucher} = \text{Base} \times \frac{2 \times \text{PRÉ}}{\text{PRÉ} + \text{VIT}_\text{cible}}
```

### Forme

La forme va de 0 à 100. Un combat la fait baisser, une défense de titre la fait baisser davantage, l'entraînement en coûte un peu et le repos en rend. Elle augmente le risque de blessure et de maladie :

```latex
\text{Risque} = \text{Risque de base} \times \left(1 + k \times \left(1 - \frac{\text{Forme}}{100}\right)\right)
```

Avec k = 2, le risque est triplé à forme nulle. La valeur de k sera réglée pendant l'équilibrage.

### Blessures

Chaque blessure a une localisation et une gravité (légère, moyenne, grave, permanente). Chaque combattant a une main dominante, ce qui donne du sens à la distinction gauche/droite.

| Localisation | Effet proposé | Cas permanent |
| --- | --- | --- |
| Tête | Baisse de précision, risque d'étourdissement accru | Séquelle : malus de précision définitif |

| Corps | Baisse de PV max et de DC | Baisse de PV définitive |
| Bras dominant | Baisse d'AC et d'AL | Perte du bras : le combattant doit changer de style |
| Bras secondaire | Baisse de DC (bouclier) | Perte du bras |
| Jambe (G ou D) | Baisse de VIT | Boiterie : VIT réduite définitivement |

### Maladies

| Niveau | Effet proposé | Durée indicative |
| --- | --- | --- |
| Faible | Petit malus sur toutes les stats | 1 à 2 semaines |
| Moyenne | Malus marqué, récupération de forme ralentie | 3 à 5 semaines |
| Sévère | Combat déconseillé, risque d'aggravation si le combattant combat | 6 semaines et plus |

### Soins

Les soins sont une décision du joueur, jamais automatique. Il peut soigner tout de suite (coût immédiat) ou attendre, par exemple pour ne pas payer une blessure légère qui guérira d'elle-même, ou pour éviter de gaspiller l'argent sur un rétablissement déjà en bonne voie.

### Personnalité, moral et mort

La personnalité est fixe (native) et définit le caractère en combat : agressivité, prudence, goût du spectacle, détermination. La détermination fixe le seuil d'abandon : un peureux abandonne tôt et meurt rarement, un déterminé continue à PV critiques et peut recevoir un coup fatal. Le joueur n'a aucun contrôle sur ce seuil.

Le moral monte avec les victoires et la renommée, baisse avec les défaites, les blessures et les maladies. Il donne un petit bonus ou malus en combat et module la progression. Il n'y a pas de loyauté.

À l'écran, la mort est représentée sobrement : une tâche de sang et une petite animation, à affiner selon ce qui est faisable.

### Renommée

Chaque combattant a une jauge de renommée qui dépend de son style et de son audace. Elle monte avec les actions spectaculaires, les victoires contre plus fort, les King of the hill et les combats sans l'équipement du colisée. Elle baisse avec les abandons, les fuites, les combats sans relief et les défis refusés. Elle augmente les recettes rapportées par le combattant.

### Progression, âge et retraite

Il n'y a pas de niveaux : les stats évoluent directement après chaque entraînement et chaque combat. Chaque combattant a un potentiel caché qui plafonne sa progression ; le recruteur aide à l'estimer.

| Âge | Effet proposé |
| --- | --- |
| 20 à 27 ans | Progression rapide |
| 28 à 35 ans | Plateau, progression lente ; retraite possible dès 30 ans |
| 36 à 39 ans | Progression très faible |
| 40 ans et plus | Déclin progressif des stats, surtout VIT |

La retraite est décidée uniquement par le combattant, selon sa personnalité, son âge et son état. Si le joueur veut s'en séparer, il le libère du club : le combattant rejoint alors le marché comme free agent.

## Le combat

Les combats se déroulent en temps réel dans une arène 2D vue de dessus, pilotés entièrement par l'IA. La même logique de combat sert à l'affichage, à la simulation invisible et à l'entraînement du machine learning.

### Classes, sorts et équipement

Chaque classe a son propre jeu de sorts, avec un temps de recharge et un coût en ressource (mana ou endurance). Pour le prototype : 2 classes et 3 sorts chacune, puis élargir. La liste définitive des classes et des sorts reste à écrire.

L'équipement est fourni par le colisée et identique pour tous. Un combattant peut refuser tout ou partie de l'équipement. Chaque pièce refusée réduit ses capacités et rapporte un bonus de renommée proportionnel au risque : combattre sans arme rapporte plus que combattre sans armure.

| Pièce refusée | Malus | Bonus de renommée |
| --- | --- | --- |
| Armure | DC et DL fortement réduites | Moyen |
| Arme | AC et AL fortement réduites | Élevé |
| Tout l'équipement | Les deux malus cumulés | Très élevé |

### Armes et étourdissement

Chaque coup frappe avec un type de dégâts, qui vient de l'arme ou du sort. Une arme contondante (bouclier, bâton, et plus tard les mains nues) étourdit plus souvent. Une arme tranchante (glaive) étourdit rarement, mais favorisera la perte d'un membre et la mort (pas encore fait). Les sorts sans élément pour l'instant (feu, éclair) n'étourdissent pas.

Plus le coup est fort, plus il risque d'étourdir, et une blessure à la tête augmente encore ce risque. Un combattant étourdi tombe : il ne fait plus rien et son adversaire attend qu'il se relève. Sa détermination décide s'il se relève, de moins en moins à chaque nouvel étourdissement du même combat. S'il se relève, le combat reprend et l'adversaire attaque de nouveau. Sinon, il est mis K.-O. : le combat s'arrête sur sa défaite, sans la honte d'un abandon, et il survit. Un combattant étourdi, relevé ou non, peut garder une blessure grave à la tête après le combat.

### Formats

| Format | Joueurs dans l'arène | Utilisé par |
| --- | --- | --- |
| Duel | 2 | Classique, Championnat du monde |
| Mêlée / King of the hill | 3 à 20, chacun pour soi | Classique |
| Équipe | 5 contre 5 | Inter-Coliseum |

En mêlée, le joueur peut inscrire plusieurs de ses combattants, mais aucune alliance n'est autorisée : l'IA traite ses coéquipiers de club comme n'importe quel adversaire.

Chaque colisée a un type d'arène qui change la tactique : sable qui ralentit, piliers qui bloquent les tirs, arène étroite qui favorise le corps à corps.

### IA de combat en deux étapes

1. **IA scriptée (utility AI).** Chaque action reçoit un score selon la situation, l'IA joue la meilleure. Elle sert à valider les règles, de secours, et d'adversaire d'entraînement pour le ML.
2. **IA apprenante (Godot RL Agents, algorithme PPO).** Un modèle par classe, entraîné en self-play, puis exporté au format ONNX et exécuté dans le jeu en C#.

Entrées du modèle : ses stats, sa forme, ses blessures, sa maladie, sa personnalité, son moral, son équipement, le type d'arène, l'état de ses ressources et sorts, puis la position et l'état des adversaires et alliés les plus proches. Limiter l'observation aux 4 adversaires et 4 alliés les plus proches permet de gérer du duel au 20 joueurs avec le même modèle.

Ordre d'entraînement conseillé : duel, puis mêlée, puis équipe. L'expérience d'un combattant peut s'exprimer par la version du modèle qu'il utilise : une recrue joue avec une version peu entraînée, un vétéran avec la plus aboutie.

### Regarder plusieurs combats en même temps

C'est faisable dans Godot avec des SubViewports : chaque combat affiché est rendu dans sa propre fenêtre, disposées en grille. Proposition : un combat en grand, les autres combats du joueur en vignettes cliquables pour changer de focus, avec vitesse x0,5, x1, x1,5 et un bouton « passer au résultat ». Seuls les combats du joueur sont affichés ; tous les autres tournent sans rendu.

### Reproductibilité

Chaque combat a une graine (seed). Pour que la même graine redonne exactement le même combat, les déplacements et collisions sont calculés par notre propre code à pas de temps fixe, pas par le moteur physique de Godot. L'IA choisit son action de façon déterministe (l'action la plus probable, le hasard passant par la graine).

## Économie, marché et personnel

L'argent vient des combats et d'un mécène, et sert à entretenir, soigner, entraîner et inscrire les combattants. Il n'y a pas de sponsor, et l'équipement est gratuit.

### Niveaux de richesse

Chaque club, celui du joueur comme les clubs rivaux, est pauvre, modeste ou riche. Le niveau fixe l'argent de départ et le don du mécène chaque semaine (plus il est riche, plus le mécène donne). Au début d'une partie, le joueur choisit un club pauvre, modeste ou riche. À chaque intersaison, le niveau suit l'argent du club, donc ses résultats et ses choix : un club monte d'un niveau s'il a assez d'argent, et redescend d'un niveau s'il en a moins de la moitié. Un club ne change que d'un niveau par saison. Le joueur ne voit pas l'argent des clubs rivaux, seulement leur niveau.

- Un club riche achète à volonté : chaque semaine, il cherche un combattant nettement meilleur que son plus faible, chez un club pas plus riche que lui, et libère son plus faible quand son effectif est plein. À l'intersaison, il libère à tout âge les combattants qui ne performent plus.
- Un club pauvre mise sur le recrutement : il engage un recruteur avant tout autre personnel, signe les rookies et les free agents du meilleur potentiel que voit son recruteur, et n'achète à personne. Il espère trouver un diamant et le revendre cher à un club plus riche.
- Un club modeste est entre les deux : il signe le free agent le plus fort, et achète à un autre club quand aucun ne lui convient.

Un club ne vend qu'à un club au moins aussi riche que lui, et n'achète qu'en gardant de quoi rester à son niveau.

### Flux d'argent

| Sens | Poste | Dépend de |
| --- | --- | --- |
| Entrée | Prime de victoire | Format et prestige du colisée |
| Entrée | Recettes du combat | Renommée des combattants, format (une mêlée à 20 attire plus) |
| Entrée | Bonus champion | Versé à chaque défense de titre réussie |
| Entrée | Vente d'un combattant | Prix négocié sur le marché |
| Entrée | Mécène | Hebdomadaire, selon le niveau de richesse du club |
| Sortie | Entretien | Coût hebdomadaire par combattant |
| Sortie | Soins | Gravité de la blessure ou maladie, niveau du médecin |
| Sortie | Entraînement | Par séance, niveau de l'entraîneur |
| Sortie | Inscription | Un frais par combattant inscrit |
| Sortie | Salaires du personnel | Hebdomadaire, une fois par tranche de 3 combattants en activité |
| Sortie | Achat d'un combattant | Prix du marché |

### Filet de sécurité

Le joueur ne peut jamais être bloqué. Le marché contient toujours des free agents disponibles, et il existe au moins un colisée de bas niveau à entrée gratuite. Même sans argent, le joueur peut donc toujours recruter un combattant et le faire combattre pour remonter la pente.

### Marché et recrutement

Un club compte au plus 3 combattants en activité ; un combattant mort ou retraité libère sa place. Le marché permet l'achat et la vente, sans échange. Il contient les free agents toute l'année (dont les combattants libérés par les clubs) et une nouvelle promotion de rookies (20 ans) à chaque intersaison. Les free agents combattent aussi dans les colisées à entrée gratuite pour montrer leur valeur : ils y gagnent un Elo, un bilan et de la renommée (pas d'argent), qu'ils gardent une fois recrutés. Les combattants sont générés procéduralement : nom, classe, stats, potentiel caché, personnalité, main dominante.

Le statut d'observation ne montre d'abord que quelques stats, sous forme de fourchettes (par exemple AC entre 40 et 60), les autres restant cachées. Chaque semaine d'observation en révèle d'autres, puis resserre les fourchettes ; le recruteur accélère et donne une estimation du potentiel.

Le prix d'un combattant dépend de ses stats, de sa renommée et de son bilan (victoires, défaites, nuls). Acheter un combattant rival passe par une négociation : le joueur fait une offre, le club répond la semaine suivante (accord, contre-offre ou refus), puis le combattant accepte ou refuse de venir. Pour vendre, le joueur met un combattant sur la liste des transferts : il reste au club et continue de combattre jusqu'à ce que le joueur accepte une offre d'un club qui a une place. Un club rival a son propre effectif, avec un maximum ; plein, il n'achète personne. Trop petit, il recrute un free agent, ou achète à un autre club si aucun free agent ne lui convient. Un combattant qui quitte un club est vendu, libéré ou retraité.

### Personnel

| Poste | Effet | Idée d'évolution |
| --- | --- | --- |
| Entraîneur | Gains d'entraînement plus élevés, moins de perte de forme à l'entraînement | Niveaux 1 à 5, salaire croissant |
| Médecin | Guérison plus rapide, moins de séquelles ; ne peut rien contre une blessure permanente (perte d'un bras) | Niveaux 1 à 5 |
| Recruteur | Fourchettes plus précises, estimation du potentiel, détection de la personnalité | Niveaux 1 à 5 |

Le personnel s'engage au marché parmi des candidats, les mêmes pour le joueur et les clubs rivaux, qui repartent au bout de quelques semaines. Chaque candidat a un niveau ; ses stats et son salaire varient autour de ceux de ce niveau. À niveau égal, l'un coûte moins cher, un autre soigne mieux mais coûte plus. Ses stats se voient sans observation.

## Colisées, classements et compétitions

Chaque colisée a son type d'arène, son prestige, son classement et son champion. Un même combattant peut figurer dans plusieurs classements et détenir plusieurs titres, au prix de défenses plus fréquentes et donc de plus de fatigue.

### Classement

Le classement utilise le système Elo, calculé séparément pour chaque colisée. Seuls les 50 meilleurs sont affichés ; les autres sont « non classés » mais gardent un Elo caché. En mêlée, chaque participant est comparé à tous les autres selon l'ordre d'élimination, ce qui adapte Elo au format jusqu'à 20 joueurs.

### Titre de champion

Le titre est distinct du rang 1. Le champion doit le défendre en duel contre un challenger du top 5 au minimum toutes les 4 semaines ; il est libre de le défendre plus souvent. Une défense coûte plus de forme qu'un combat normal et rapporte le bonus champion.

### Défis

Les clubs peuvent proposer et recevoir des défis. Refuser un défi n'est pas sanctionné en soi, mais un combattant qui refuse trop et ne combat pas assez perd de la renommée et de l'Elo.

### Les trois compétitions

| Compétition | Fréquence | Participants | Format |
| --- | --- | --- | --- |
| La Classique | Toute la saison, dans chaque colisée | Tous les inscrits | Duel ou mêlée (de 2 à 20), compte pour le classement |
| Championnat du monde | Saisons 4, 8, 12… | Top 5 de chaque colisée | Duels : phase de poules puis élimination directe |
| Inter-Coliseum | Saisons impaires | 5 combattants convoqués par colisée | 5 contre 5, colisée contre colisée |

Pour l'Inter-Coliseum, chaque colisée convoque ses 5 meilleurs selon son classement. Un combattant convoqué par deux colisées choisit : c'est le joueur qui décide pour ses combattants, et la personnalité décide pour les autres. Le colisée délaissé convoque alors le suivant de son classement.

Le nombre de participants au championnat du monde dépend du nombre de colisées : avec 8 colisées, on obtient 40 combattants, soit 8 poules de 5.

## Le monde : managers rivaux et simulation

Les clubs rivaux sont gérés par une IA à règles, et leurs combats sont résolus par un modèle statistique calibré sur des milliers de vrais combats simulés. Les combats impliquant un combattant du joueur sont toujours de vrais combats simulés.

### IA des managers rivaux

Chaque club rival a un profil (prudent, agressif, marchand) qui règle ses seuils. Chaque semaine, pour chaque combattant, il applique des règles simples :

- Forme sous le seuil ou blessure : repos, et soins si le budget le permet et si la blessure le justifie.
- Forme correcte : inscription au colisée où ses chances sont les meilleures, sinon entraînement.
- Défis : en proposer à des adversaires d'Elo proche, accepter ceux du joueur si la chance de victoire estimée dépasse son seuil.
- Marché : acheter si l'effectif est trop petit et le budget suffisant, libérer ou vendre les combattants âgés ou en déclin, selon son niveau de richesse (voir « Niveaux de richesse »).
- Personnel : engager parmi les candidats du marché le meilleur entraîneur puis le meilleur médecin (le moins cher à niveau égal) dont il peut payer les salaires assez longtemps (plus ou moins selon son profil), et les garder tant qu'il peut encore les payer moitié moins longtemps, sauf si un candidat d'un meilleur niveau se présente. Leurs effets valent pour ses combattants comme pour ceux du joueur. Un club pauvre engage d'abord un recruteur, pour voir le potentiel des rookies et des free agents ; les autres n'en engagent pas, jugeant un combattant sur ce qu'il montre.

### Simulation en arrière-plan

Avec par exemple 8 colisées et environ 500 combattants, il y a plusieurs centaines de combats par semaine. Les simuler tous en temps réel serait trop lent, d'où deux niveaux de résolution :

| Combat | Résolution |
| --- | --- |
| Implique un combattant du joueur | Vrai combat simulé (affiché ou non) |
| Entre rivaux | Modèle statistique calibré |

La calibration se fait hors du jeu : on lance des milliers de combats sans rendu, on enregistre les résultats, puis on ajuste un modèle qui prédit le vainqueur, les dégâts, les blessures et les morts à partir des stats et de l'état des combattants. Une régression logistique suffit pour le vainqueur, et c'est déjà du machine learning. On vérifie ensuite que les taux de victoire, de blessure et de mort sont les mêmes dans les deux modes.

## Interface, comptes rendus, sauvegarde et tutoriel

L'interface compte huit écrans ; dans un jeu de management, elle représente souvent la moitié du travail et doit être planifiée comme telle.

### Écrans

| Écran | Contenu principal |
| --- | --- |
| Menu général | Semaine en cours, argent, alertes (blessures, défis reçus), bouton « semaine suivante » |
| Colisées | Liste des colisées, type d'arène, champion, top 50, inscriptions |
| Liste des combattants | Effectif avec forme, état, rang ; choix repos, entraînement ou soins en un clic |
| Fiche combattant | Stats, historique, blessures, maladie, personnalité, moral, renommée, âge ; bouton « libérer » |
| Marché | Rookies et free agents, fourchettes d'observation, achat et vente |
| Personnel | Entraîneur, médecin, recruteur : niveau, salaire, embauche |
| Combat | Combat principal et vignettes des autres, contrôles de vitesse |
| Résumé de combat | Compte rendu détaillé |

### Compte rendu de combat

Pour chaque combattant : dégâts infligés, dégâts reçus, sorts utilisés, blessures reçues, argent gagné, renommée gagnée ou perdue. On y ajoute la variation de forme et d'Elo, et la graine du combat pour pouvoir le revoir.

### Sauvegarde

Une sauvegarde automatique après chaque combat regardé, plus une sauvegarde manuelle. Format : un fichier JSON avec un numéro de version, pour que les anciennes sauvegardes restent lisibles quand le jeu évolue. Les graines permettent de ne stocker que les résultats, pas les combats entiers.

### Tutoriel et manuel

Chaque fonctionnalité est débloquée progressivement, présentée par une pop-up à sa première apparition, puis inscrite dans le manuel du jeu. Ordre de déblocage proposé : combat et compte rendu, repos et entraînement, blessures et soins, marché, personnel, défis, mêlées, titres de champion, puis les grands événements.

## Architecture technique Godot

Le projet part sur Godot 4.7.2 en version .NET (C#), indispensable pour exécuter les modèles ONNX de Godot RL Agents. La règle d'or : la logique du jeu est du C# pur, séparée de l'affichage, pour pouvoir simuler sans rendu, entraîner l'IA et tester.

```mermaid
flowchart TD
  D[Données<br/>combattants, colisées, clubs] --> W[Logique du monde<br/>calendrier, économie, marché]
  W --> C[Moteur de combat<br/>C# pur, pas de temps fixe]
  C --> A[IA de combat<br/>scriptée ou ONNX]
  W --> R[IA des rivaux<br/>règles]
  C --> V[Affichage<br/>scènes, sprites, SubViewports]
  W --> U[Interface<br/>8 écrans]
```

L'affichage ne fait que lire l'état du moteur de combat à chaque image : il ne décide jamais de rien. Le même moteur tourne donc à l'écran, en arrière-plan et pendant l'entraînement.

### Modules

| Module | Responsabilité |
| --- | --- |
| GameState (autoload) | État global de la partie, point d'entrée de la sauvegarde |
| Calendar | Semaines, saisons, grands événements, intersaison |
| FighterSystem | Forme, blessures, maladies, soins, progression, âge, retraite, mort |
| CombatEngine | Déplacements, collisions, dégâts, sorts, équipement, graine aléatoire |
| CombatAI | Interface commune : utility AI ou modèle ONNX |
| StatModel | Résolution statistique des combats entre rivaux |
| RivalAI | Décisions hebdomadaires des clubs rivaux |
| Economy, Market, Staff | Argent, achats, ventes et libérations, personnel |
| Rankings | Elo par colisée, titres, défis, compétitions |
| SaveSystem | JSON versionné, auto et manuel |
| Achievements | Succès du jeu, reliés aux succès Steam |
| UI | Écrans, pop-ups, manuel |

### Environnement d'entraînement

Une scène dédiée fait tourner plusieurs dizaines d'arènes en parallèle, en mode sans rendu et accéléré. Côté Python, Godot RL Agents pilote l'entraînement avec Stable-Baselines3. Les stats, blessures, personnalités, équipements et arènes sont tirés au hasard à chaque combat pour que le modèle s'adapte à tout.

## Outils et environnement

Le poste de travail tient en cinq outils gratuits : Godot 4.7.2 .NET, le SDK .NET, JetBrains Rider, Git avec Git LFS, et un dépôt GitHub. Ni serveur de base de données ni Docker au départ.

| Besoin | Choix | Remarque |
| --- | --- | --- |
| Moteur | Godot 4.7.2 .NET | Figer la version pendant la production, n'appliquer que les correctifs 4.7.x |
| IDE | JetBrains Rider | Gratuit en usage non commercial ; alternative : VS Code + C# Dev Kit + godot-tools |
| Versionnage | Git + GitHub | Branche `main` toujours fonctionnelle, une branche par fonctionnalité, un tag par phase |
| Fichiers binaires | Git LFS | Sprites, sons, fichiers Aseprite ; surveiller le quota gratuit |
| Données de design | Resources Godot (`.tres`) ou JSON | Classes, sorts, arènes, noms pour la génération |
| Sauvegardes | JSON avec `System.Text.Json` | Dans `user://`, compressé en GZip si besoin |
| Historique volumineux | SQLite (Microsoft.Data.Sqlite) | Seulement si le JSON devient trop lourd |
| Intégration Steam | Steamworks.NET ou Facepunch.Steamworks | Succès Steam, sauvegarde cloud |
| Tests | xUnit ou NUnit sur la logique C# pure | Rendu possible par la séparation logique / affichage |
| Entraînement ML | Environnement virtuel Python (venv ou conda) | Plus simple que Docker, surtout sous Windows |
| Intégration continue | GitHub Actions + image Godot headless | Optionnel, plus tard : build, tests, envoi sur une branche bêta Steam |

### Git avec Godot

À versionner : `project.godot`, les scènes (`.tscn`), les ressources (`.tres`), les scripts C#, le `.csproj`, le `.sln`, les fichiers `.import` et les fichiers `.uid` (sans eux, les références aux scripts cassent). À ignorer :

```
.godot/
bin/
obj/
.vs/
.idea/
*.user
export/
```

Ajouter un `.gitattributes` avec `* text=auto eol=lf` pour éviter les soucis de fins de ligne. Éviter de modifier la même scène dans deux branches en parallèle, car les conflits dans les `.tscn` sont pénibles à résoudre.

### Support et communauté

Documentation officielle, forum officiel, Discord officiel et r/godot pour Godot ; la communauté Chickensoft pour Godot en C# ; le Discord et les issues GitHub de Godot RL Agents pour la partie ML.

## Feuille de route

Le développement se fait en neuf phases, chacune terminée par un jalon jouable. À la fin de la phase 4, le jeu est complet avec l'IA scriptée ; le machine learning arrive ensuite comme amélioration.

| Phase | Contenu | Jalon (comment savoir que c'est fini) |
| --- | --- | --- |
| 0. Préparation | Godot 4.7.2 .NET, Rider, dépôt Git avec LFS, sprites gratuits (Kenney.nl, OpenGameArt) | Un projet vide qui compile en C# et se lance depuis Rider |
| 1. Combat de base | Moteur de combat en duel, 2 classes, 3 sorts chacune, utility AI, graines, compte rendu simple | Deux combattants s'affrontent ; la même graine redonne le même combat |
| 2. Le combattant complet | Forme, blessures, maladies, soins, personnalité, seuil d'abandon, mort, progression, âge, retraite | Un combattant enchaîne 10 semaines avec des effets visibles |
| 3. Management | Calendrier, économie, filet de sécurité, menu général, liste et fiche combattant, sauvegarde | Une saison entière jouable avec un seul club |
| 4. Le monde | Colisées, Elo, titres, défis, clubs rivaux, marché, personnel, modèle statistique calibré | Plusieurs saisons où les classements vivent sans le joueur |
| 5. Formats et spectacle | Mêlée jusqu'à 20, King of the hill, équipement refusable, vue multi-combats, contrôles de vitesse | Regarder 4 combats du joueur en même temps |
| 6. IA apprenante | Godot RL Agents, entraînement en duel, puis mêlée, puis équipe, export ONNX | Le modèle appris bat nettement l'IA scriptée |
| 7. Grands événements | Championnat du monde, Inter-Coliseum en 5 contre 5, succès, tutoriel et manuel | Un cycle complet de 4 saisons avec les deux événements |
| 8. Contenu et équilibrage | Nouvelles classes et sorts, arènes, équilibrage par simulations de masse, finitions | Taux de victoire par classe entre 45 et 55 % |
| 9. Publication Steam | Compte Steamworks, page du magasin, succès Steam, builds Windows et Linux, test Steam Deck | Le jeu est disponible gratuitement sur Steam |

## Décisions prises et points ouverts

### Décisions prises

| Sujet | Décision |
| --- | --- |
| Filet de sécurité | Toujours des free agents disponibles, et un colisée de bas niveau à entrée gratuite |
| Retraite | Décidée uniquement par le combattant ; le joueur peut le libérer du club |
| Soins | Choisis par le joueur, jamais automatiques |
| Durée de la compétition | 25 semaines par saison |
| Titre de champion | Défense au minimum toutes les 4 semaines, plus souvent si le champion le souhaite |
| Combat sans équipement | Malus et bonus selon la pièce refusée ; refuser l'arme rapporte plus que refuser l'armure |
| Refus de défi | Perte de renommée et d'Elo si le combattant ne combat pas assez |
| Mêlée | Plusieurs combattants d'un même club autorisés, sans aucune alliance |
| Mort à l'écran | Tâche de sang et petite animation, selon ce qui est faisable |

### Points encore ouverts

- [ ] Taille du monde : nombre de colisées et de clubs rivaux.
- [ ] Liste des classes et de leurs sorts.
- [ ] Calendrier des grands événements : confirmer l'Inter-Coliseum aux saisons impaires, ou revenir à la règle du report.

### Risques

| Risque | Conséquence | Parade |
| --- | --- | --- |
| Équilibrage difficile entre classes, sorts et malus | Une classe domine, les autres sont inutiles | Mode de simulation de masse dès la phase 1, suivi des taux de victoire |
| Entraînement ML lent ou instable | Phase 6 qui s'éternise | Jeu complet avec l'IA scriptée ; self-play contre d'anciennes versions et contre l'IA scriptée |
| Maintenance ralentie de Godot RL Agents | Incompatibilité avec Godot 4.7 | Tester tôt sur un mini-projet ; garder l'IA derrière une interface commune pour pouvoir la désactiver |
| Récompenses mal conçues | Comportements absurdes (fuite permanente) | Récompenser dégâts et victoire, légère pénalité de temps, revoir les combats régulièrement |
| Performances en mêlée à 20 avec vue multi-combats | Ralentissements | IA appelée quelques fois par seconde, pas à chaque image ; limiter le nombre de vignettes |
| Écart entre modèle statistique et vrais combats | Résultats incohérents entre joueur et rivaux | Recalibrer après chaque changement d'équilibrage |
| Interface sous-estimée | Retard général | L'inclure explicitement dans chaque phase |
| Portée trop large pour un développeur seul | Projet jamais terminé | Respecter l'ordre des phases, 2 classes et 2 colisées au départ |
