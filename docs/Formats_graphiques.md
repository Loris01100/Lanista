# Formats graphiques et sonores

Ce document fixe les formats des sprites, arènes, effets et sons du jeu, et la façon de les exporter depuis Aseprite. Il complète la [feuille de route](feuille_de_route/README.md) (phase 6 : apparence aléatoire des combattants).

## Règles communes

- PNG RGBA, fond transparent.
- Pixel art sans anticrénelage : le jeu affiche les textures en filtrage `Nearest`, un bord adouci devient un halo.
- Échelle unique : **1 m = 20 px**. Combattants, sols, taches et décors sont dessinés à cette échelle ; le jeu agrandit l'ensemble à l'affichage d'un facteur entier (voir « Agrandissement »).
- Palette limitée (16 à 32 couleurs, par exemple « Endesga 32 » sur Lospec), commune à tout le jeu.
- Les fichiers exportés vont dans `assets/` (chargés par le jeu) ; les fichiers sources `.aseprite` vont dans `art/` (non livrés). Git LFS suit les deux.

## Combattants

### Case et orientation

- Une image fait **32 × 32 px**, soit 1,6 m, la taille d'affichage du combattant.
- Le corps fait environ 20 px de large (1 m, le diamètre du combattant en combat). Le centre de la case est la position du combattant.
- Le combattant regarde vers la droite ; le jeu le retourne quand il regarde à gauche.

### Animations

Une spritesheet de **4 colonnes × 4 lignes, 128 × 128 px** : une ligne par animation, 4 images par animation, dans cet ordre.

| Ligne | Tag Aseprite | Animation | Durée par image | Boucle |
| --- | --- | --- | --- | --- |
| 0 | `idle` | Repos | 0,25 s | oui |
| 1 | `move` | Déplacement | 0,1 s | oui |
| 2 | `attack` | Attaque ou sort | 0,06 s | non, l'impact est la 3ᵉ image |
| 3 | `death` | Mort | 0,15 s | non, la dernière image est le corps au sol |

Les durées sont celles du jeu ; les mettre aussi dans Aseprite pour que l'aperçu soit fidèle.

### Calques

Le personnage est découpé en calques superposés, un fichier par calque :

| Calque | Ordre de dessin | Teinté |
| --- | --- | --- |
| `leg_back` (jambe arrière) | 1 | peau |
| `arm_back` (bras arrière) | 2 | peau |
| `body` (corps) | 3 | tunique |
| `leg_front` (jambe avant) | 4 | peau |
| `head` (tête) | 5 | peau |
| `hair` (cheveux) | 6 | cheveux |
| `arm_front` (bras avant) | 7 | peau |

- Une seule teinte par fichier : la tête et les cheveux sont donc deux calques, et ce qui, sur un calque, n'a pas sa teinte (une jambe nue sous la tunique, un col…) va dans un autre calque ou dans le fichier de détail.
- Chaque calque de corps et de membre est une feuille complète de 128 × 128 px sur la même grille, transparente hors de sa partie.
- La tête et les cheveux sont chacun **une seule image de 32 × 32 px** par variante, placée comme dans l'image de repos. Le jeu les décale et les tourne image par image autour du cou pour suivre le corps : le point de rotation (`head_pivot`) et, pour chacune des 16 images, le décalage en pixels et l'angle en degrés dans le sens horaire (`head_frames`) sont dans `appearance`, dans `data/fighter.ron`. Une nouvelle carrure dont le cou bouge autrement demande d'y reporter ses valeurs.
- « Avant » et « arrière » sont vus du combattant tourné vers la droite. Le côté anatomique (gauche ou droit, main dominante) est géré par le jeu.

### Variantes et teintes

- Une variante par **forme** seulement (chauve, cheveux courts, longs, crête…), jamais une par couleur.
- Les parties qui changent de couleur (peau, cheveux, tunique) sont dessinées en **niveaux de gris clairs**. Le jeu multiplie le dessin par une couleur : le blanc donne la couleur pure, les gris ses ombres.
- La teinte s'applique à tout un fichier. Ce qui garde sa couleur (yeux, métal, cuir, contours sombres) va dans un **fichier de détail non teinté** du même calque, suffixé `_detail`.
- Pour débuter : 2 formes de cheveux et 3 teintes.

### Fichiers

```text
art/fighters/<variante>.aseprite          source (un fichier par carrure ou par tête)
assets/fighters/<calque>/<variante>.png   teinté
assets/fighters/<calque>/<variante>_detail.png   non teinté, facultatif
```

Les variantes de chaque calque sont la carrure (`build` dans `data/fighter.ron`, `average` pour l'instant), sauf pour les cheveux, une variante par forme (`hair`). Exemples : `assets/fighters/head/average.png`, `assets/fighters/head/average_detail.png` (les yeux), `assets/fighters/hair/long.png`, `assets/fighters/body/average.png`.

Les calques actuels sont **provisoires** : des formes simples en gris clair (carrure `average`, cheveux `short` et `long`), générées par un script en attendant les vrais dessins, qui les remplacent sous les mêmes noms.

### Personnages uniques

Un personnage unique est dessiné à la main, sans calques générés ni teintes :

- Même case (32 × 32 px), même orientation, mêmes 4 animations, même feuille de 128 × 128 px que les autres. Il peut ajouter ses propres animations en lignes supplémentaires, une fois que le jeu lira les animations depuis `data/`.
- Fichiers : `art/uniques/<nom>.aseprite` et `assets/uniques/<nom>.png`, une seule feuille exportée avec tous les calques visibles.
- Dans `lanista_core`, son `Appearance` vaut `Unique(<nom>)` au lieu des indices de forme et de teinte ; `lanista` dessine alors cette feuille seule. Blessures et équipement restent des calques par-dessus, comme pour les autres.

## Arènes

- Sol d'un duel : une image par type d'arène, à 20 px/m : 20 × 20 m, soit **400 × 400 px**, pour le sable et les piliers ; 8 × 6 m, soit **160 × 120 px**, pour l'arène étroite (`arenas` dans `data/combat.ron`).
- En mêlée, le sol grandit avec le nombre de combattants (30 m² chacun, `melee` dans `data/combat.ron`) : environ 24,5 m de côté à 20, soit près de 490 px, piliers compris. Le sol d'une mêlée doit donc se répéter ou s'étirer sans se voir, ou exister en plusieurs tailles.
- Autour du sol, le jeu montre 1 m de marge puis des gradins jusqu'au bord de l'écran, dont la largeur dépend de l'écran et de l'agrandissement (64 × 40 m en tout sur un écran de 1280 × 800 px, autour d'un duel de 20 m). Les gradins sont une **texture répétable**, par exemple 64 × 64 px.

## Effets

- Tache de sang : 1,4 m de diamètre, environ **28 × 28 px**. Le jeu la fait grandir.
- Projectiles et sorts : cases de 16 × 16 ou 32 × 32 px, même échelle.
- Mine au sol : 0,6 m de diamètre (`radius` de `mines` dans `data/combat.ron`), soit une case de **16 × 16 px**, en deux images : posée, puis armée. Le jeu la dessine pour l'instant en cercles. Son explosion : 3 m de diamètre (`blast_radius`), soit **64 × 64 px**, à venir.

## Interface

- L'interface (egui) est conçue pour 800 px de haut et agrandie au-delà ; icônes de 16 ou 32 px.
- Le portrait de la fiche combattant réutilise les calques de tête, agrandis ×4.
- Polices : TTF ou OTF.

## Sons

- Effets en WAV, musique en OGG (jouée en boucle), 44,1 kHz.

## Aseprite

### Créer le fichier

Le script `art/scripts/fighter_template.lua` crée ce fichier pour une carrure (`art/fighters/<carrure>.aseprite`) : images, tags, durées et calques comme ci-dessous, remplis avec les dessins déjà présents dans `assets/fighters/` (la tête et les cheveux sur la première image seulement), la palette `art/palette.gpl` si elle existe, et un calque verrouillé `_guide` (centre de la case et cercle de 1 m) que l'exportation ignore, comme tout calque dont le nom commence par `_`. À faire à la main sinon :

- Nouveau sprite de 32 × 32 px, mode de couleur RGBA, fond transparent.
- Charger la palette du jeu (Palette > Load Palette).
- Un calque par partie, nommé comme dans le tableau des calques (`leg_back`, `arm_back`, `body`, `leg_front`, `head`, `hair`, `arm_front`), plus un calque `_detail` à côté de chaque partie qui en a.
- 16 images ; un tag par animation (`idle` images 1 à 4, `move` 5 à 8, `attack` 9 à 12, `death` 13 à 16).
- Utiliser le papier calque (Onion Skin) pour animer.

Dessiner d'abord un personnage complet, puis le répartir en calques à la place des calques provisoires de `assets/fighters/` (même grille, mêmes noms) et créer les variantes.

### Exporter

File > Export Sprite Sheet, une exportation par calque :

- Layout : Sheet Type « By Rows », Constraints « Fixed # of Columns » = 4.
- Sprite : Layers = le calque à exporter ; Frames = toutes.
- Borders : tout à 0 (pas de marge ni d'espacement entre les images).
- Output : Output File = `assets/fighters/<calque>/<variante>.png` ; décocher JSON Data.

Pour la tête et les cheveux, exporter seulement la première image des calques `head` et `hair` (File > Export As, Frames = 1).

## Agrandissement

Le combat en grand est agrandi d'un facteur entier, le plus grand qui montre toute l'arène et sa marge (`reesch::pixel_view`) : chaque pixel d'un dessin couvre le même nombre de pixels d'écran. Autour d'un duel de 20 m, cela donne ×1 sur un écran de 800 px de haut (Steam Deck), ×2 en 1080p et ×3 en 1440p ; le reste de l'écran montre plus de gradins. Un écran trop petit pour montrer l'arène à ×1 la réduit sans facteur entier, comme les vignettes de la vue multi-combats. Cela ne change rien aux dessins.
