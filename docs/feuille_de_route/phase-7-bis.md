# Phase 7 bis — Le ludus

Une fois riche, un club n'a plus grand-chose à acheter : sans sponsor ni équipement payant, son argent s'accumule. Cette phase lui donne une dépense de long terme et une identité : le ludus, un plan de terrains où le club bâtit selon ce qu'il veut privilégier. Les règles du jeu sont dans la section « Ludus » du [plan de conception](../Gladiator_Manager_Plan_de_conception.md) ; cette phase ne dépend pas de l'IA apprenante (phase 7) et vient avant l'équilibrage (phase 9), qui réglera ses chiffres.

**Étape 1 — Le plan et les bâtiments, en données.**
- Un nouveau fichier `data/ludus.ron` : les bâtiments (nom, famille, prix, entretien, durée de construction, effet), les familles et leur spécialité, la part de l'effet de chaque exemplaire (100, 60 puis 30 %, trois au plus), le nombre de terrains au départ et le prix des parcelles, qui monte à chacune. Ajouter un bâtiment ne demande que des données, sauf s'il agit sur quelque chose qu'aucun bâtiment ne touche encore.
- Chaque club a son ludus dans `GameState` : une liste de terrains, chacun libre ou portant un bâtiment, construit ou en construction (semaines restantes). Pas de niveau.
- Les décisions de la semaine gagnent trois coups : construire un bâtiment sur un terrain libre, démolir, acheter une parcelle. La construction et la parcelle sont payées à la décision, l'entretien de chaque bâtiment debout chaque semaine, même à crédit, comme les salaires. Démolir ne rembourse rien.
- Le chargement refuse un bâtiment d'une famille inconnue, une part d'exemplaire hors de 0 à 100 % et un ludus sans terrain.

**Étape 2 — Les effets.**
- Chaque bâtiment agit sur une règle qui existe déjà : gains d'entraînement, progression des moins de 23 ans, vitesse de guérison (seulement avec un médecin), forme regagnée au repos, moral, entretien des combattants, moral avant une finale ou une défense de titre.
- L'effet total d'un bâtiment est la somme de ses exemplaires construits, chacun à sa part. Un bâtiment en construction n'agit pas.

**Étape 3 — L'écran.**
- Un onglet « Ludus » après « Personnel » : le plan des terrains (bâtiment et rang de l'exemplaire, terrain libre, parcelle voisine à acheter, construction en cours), la fiche du terrain choisi (effet total et ce qu'apporterait un exemplaire de plus, entretien, démolir), le catalogue d'un terrain libre, l'avancement de chaque famille vers sa spécialité, et « Cette semaine » avec un bouton Annuler par coup, comme le personnel.
- Les alertes du menu général annoncent une construction finie. La caisse prévue compte les constructions, les parcelles et l'entretien.

**Étape 4 — Les spécialités.**
- Trois bâtiments d'une même famille donnent au club un titre, perdu si l'un est démoli : École renommée (entraînement), Ludus des médecins (soins), et un troisième pour l'intendance, à préciser.
- Chaque titre agit sur le monde plutôt que d'ajouter un pourcentage : les rookies rejoignent plus volontiers une École renommée et le recruteur y voit mieux leur potentiel ; un Ludus des médecins laisse moins de séquelles.
- Le titre s'affiche à côté du nom du club, sur la page d'un club rival et dans l'Acta Diurna quand un club le gagne.

**Étape 5 — Les clubs de l'ordinateur bâtissent.**
- Chaque club bâtit selon sa richesse : un club pauvre mise sur l'entraînement et la Schola, pour former puis revendre ; un club riche sur les soins ; un club modeste entre les deux. Il ne bâtit qu'en gardant de quoi rester à son niveau.
- Le modèle statistique des combats entre clubs de l'ordinateur tient compte des bâtiments : `calibrate` puis `compare` après cette étape. Le jalon de la phase 5 et l'outil `temperaments` doivent tenir.

**Points encore ouverts.**
- Le nom de l'écran : les clubs ne s'appellent pas tous « Ludus » (« Palestre de… », « Ordu de… », phase 6 bis). Garder « Ludus » partout, ou suivre la culture du club ; et renommer le bâtiment Palestre s'il prête à confusion avec le gabarit « Palestre de {name} ».
- Les chiffres : terrains au départ, nombre et prix des parcelles, liste des bâtiments, prix, durées et effets.

**Jalon :** trois saisons où le joueur bâtit une École renommée sur plusieurs terrains, puis en démolit un et perd le titre, pendant qu'un club pauvre et un club riche de l'ordinateur bâtissent des ludus différents.
