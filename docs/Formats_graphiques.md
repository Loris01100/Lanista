# Formats graphiques et sonores

Ce document fixe les formats des sprites, arènes, effets et sons du jeu, et la façon de les exporter depuis Aseprite. Il complète la [feuille de route](Gladiator_Manager_Feuille_de_route_Rust.md) (phase 6 : apparence aléatoire des combattants).

## Règles communes

- PNG RGBA, fond transparent.
- Pixel art sans anticrénelage : le jeu affiche les textures en filtrage `Nearest`, un bord adouci devient un halo.
- Échelle unique : **1 m = 20 px**. Combattants, sols, taches et décors sont dessinés à cette échelle ; le jeu agrandit l'ensemble à l'affichage.
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
| `leg_back` (jambe arrière) | 1 | peau ou tunique |
| `arm_back` (bras arrière) | 2 | peau |
| `body` (corps) | 3 | peau, tunique |
| `leg_front` (jambe avant) | 4 | peau ou tunique |
| `head` (tête et cheveux) | 5 | peau, cheveux |
| `arm_front` (bras avant) | 6 | peau |

- Chaque calque de corps et de membre est une feuille complète de 128 × 128 px sur la même grille, transparente hors de sa partie.
- La tête est **une seule image de 32 × 32 px** par variante, placée comme dans l'image de repos. Le jeu la décale image par image pour suivre le corps ; ce décalage sera réglé dans `data/`.
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

Exemples : `assets/fighters/head/bald.png`, `assets/fighters/head/bald_detail.png`, `assets/fighters/body/average.png`.

### Personnages uniques

Un personnage unique est dessiné à la main, sans calques générés ni teintes :

- Même case (32 × 32 px), même orientation, mêmes 4 animations, même feuille de 128 × 128 px que les autres. Il peut ajouter ses propres animations en lignes supplémentaires, une fois que le jeu lira les animations depuis `data/`.
- Fichiers : `art/uniques/<nom>.aseprite` et `assets/uniques/<nom>.png`, une seule feuille exportée avec tous les calques visibles.
- Dans `lanista_core`, son `Appearance` vaut `Unique(<nom>)` au lieu des indices de forme et de teinte ; `lanista` dessine alors cette feuille seule. Blessures et équipement restent des calques par-dessus, comme pour les autres.

## Arènes

- Sol : 20 × 20 m, soit **400 × 400 px**, une image par type d'arène. La taille des arènes de mêlée sera fixée en phase 6, toujours à 20 px/m.
- Autour du sol, le jeu montre 1 m de marge puis des gradins jusqu'au bord de l'écran, dont la largeur dépend du format (environ 35 m en 16:10, davantage en ultra-large). Les gradins sont une **texture répétable**, par exemple 64 × 64 px.

## Effets

- Tache de sang : 1,4 m de diamètre, environ **28 × 28 px**. Le jeu la fait grandir.
- Projectiles et sorts : cases de 16 × 16 ou 32 × 32 px, même échelle.

## Interface

- L'interface (egui) est conçue pour 800 px de haut et agrandie au-delà ; icônes de 16 ou 32 px.
- Le portrait de la fiche combattant réutilise les calques de tête, agrandis ×4.
- Polices : TTF ou OTF.

## Sons

- Effets en WAV, musique en OGG (jouée en boucle), 44,1 kHz.

## Aseprite

### Créer le fichier

- Nouveau sprite de 32 × 32 px, mode de couleur RGBA, fond transparent.
- Charger la palette du jeu (Palette > Load Palette).
- Un calque par partie, nommé comme dans le tableau des calques (`leg_back`, `arm_back`, `body`, `leg_front`, `head`, `arm_front`), plus un calque `_detail` à côté de chaque partie qui en a.
- 16 images ; un tag par animation (`idle` images 1 à 4, `move` 5 à 8, `attack` 9 à 12, `death` 13 à 16).
- Utiliser le papier calque (Onion Skin) pour animer.

Dessiner d'abord un personnage complet et le tester dans le jeu à la place de `assets/placeholder/fighter.png` (même grille), puis seulement ensuite le répartir en calques et créer les variantes.

### Exporter

File > Export Sprite Sheet, une exportation par calque :

- Layout : Sheet Type « By Rows », Constraints « Fixed # of Columns » = 4.
- Sprite : Layers = le calque à exporter ; Frames = toutes.
- Borders : tout à 0 (pas de marge ni d'espacement entre les images).
- Output : Output File = `assets/fighters/<calque>/<variante>.png` ; décocher JSON Data.

Pour la tête, exporter seulement la première image du calque `head` (File > Export As, Frames = 1).

## Point ouvert

Sur un écran de 800 px de haut, l'arène est affichée à environ 36 px/m, soit ×1,8 : un agrandissement non entier donne des pixels de tailles inégales. Caler l'agrandissement sur un entier est prévu avec les render targets de la phase 6 ; cela ne change rien aux dessins.
