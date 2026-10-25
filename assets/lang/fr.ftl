# Textes du jeu en français (Fluent : https://projectfluent.org).
# Une variante choisie par une clé anglaise ($role, $side, $stronger…) suit le code ;
# {""} est une variante vide.

## Combattants

stat-hp = PV
stat-melee-attack = AC
stat-ranged-attack = AL
stat-accuracy = PRÉ
stat-speed = VIT
stat-melee-defense = DC
stat-ranged-defense = DL

fighter-name = { $name } ({ $class })
fighter-unnamed = { $class } n° { $number }
fighter-numbered = { $name } { $number }
free-agent-name = { $name } ({ $class }, free agent)
free-agent-unnamed = Free agent n° { $number } ({ $class })
rival-fighter-name = { $name } ({ $class }, { $club })
rival-fighter-unnamed = { $class } n° { $number } ({ $club })
contender-yours = { $fighter } ({ $club })

condition-dead = mort
condition-retired = retraité
condition-sold = vendu
condition-released = libéré
condition-injured = blessé
condition-ill = malade
condition-fit = en forme

trait-aggression = Agressivité
trait-caution = Prudence
trait-showmanship = Goût du spectacle
trait-determination = Détermination
personality-short = Agr { $aggression }, Pru { $caution }, Spe { $showmanship }, Dét { $determination }
record = { $wins } V, { $losses } D, { $draws } N
record-wins = { $count } V
record-losses = { $count } D
record-draws = { $count } N

form = Forme
morale = moral
fame = renommée

## Blessures et maladies

location-head = Tête
location-body = Corps
location-arm = Bras { $side ->
        [left] gauche
       *[right] droit
    }
location-arm-dominant = Bras { $side ->
        [left] gauche
       *[right] droit
    } (dominant)
location-leg = Jambe { $side ->
        [left] gauche
       *[right] droite
    }

weeks = { $n ->
        [1] 1 semaine
       *[other] { $n } semaines
    }

injury = { $location } : { $severity ->
        [light] légère
        [medium] moyenne
       *[severe] grave
    }, { $weeks }
injury-permanent = { $location } : séquelle définitive
injury-lost = { $location } : { $leg ->
        [true] perdue
       *[false] perdu
    }

illness = Maladie { $level ->
        [mild] faible
        [moderate] moyenne
       *[severe] sévère
    }, { $weeks }

cut-head = tête tranchée
cut-body = corps tranché
cut-arm = bras { $side ->
        [left] gauche
       *[right] droit
    } tranché
cut-leg = jambe { $side ->
        [left] gauche
       *[right] droite
    } tranchée

## Semaine

activity-rest = Repos
activity-train = Entraînement
activity-fight = Combat à { $place }
activity-title-fight = Combat pour le titre à { $place }
activity-challenge = Défi à { $place }
activity-melee = Mêlée à { $place }

week-injury = Blessure : { $injury }
week-after-effect-spared = Séquelle évitée par le médecin
week-healed = Guéri : { $location }
week-ill = Tombe malade : { $illness }
week-illness-healed = Maladie guérie
week-birthday = Un an de plus : { $age } ans
week-season-gains = Bilan de la saison : { $gains } progrès
week-stats = Stats : { $stats }
week-retires-maimed = Prend sa retraite : trop mutilé pour combattre
week-retires = Prend sa retraite à { $age } ans

date = Saison { $season }, semaine { $week }
date-in-sentence = saison { $season }, semaine { $week }
year = Anno { $year } ab Urbe condita
detailed = { $what } : { $details }

## Combats

result-beheaded = décapité
result-dead = mort au combat
result-severed = défaite, { $limb }
result-left-for-dead = laissé pour mort, survit grièvement blessé
result-knocked-out = défaite, étourdi sans se relever
result-won = victoire
result-lost = défaite
result-gave-up = abandon
result-unfinished = pas de vainqueur après { $minutes ->
        [1] une minute
       *[other] { $minutes } minutes
    }

against-duel = Contre un { $class }{ $stronger ->
        [true] , plus fort
       *[false] {""}
    }
against-melee = Mêlée à { $size }, { $place }{ $stronger ->
        [true] , plus forts
       *[false] {""}
    }
fight-against = { $against }{ $refused ->
        [armour] , sans armure
       *[none] {""}
    }
fight = { $against } : { $result }, renommée { $fame }

place = { $place ->
        [1] 1er
       *[other] { $place }e
    }

## Argent

money = Argent
flow-upkeep = entretien
flow-care = soins
flow-training = entraînement
flow-entry-fee = inscription
flow-win-prize = prime de victoire
flow-receipts = recettes
flow-champion-bonus = bonus champion
flow-purchase = achat
flow-sale = vente
flow-salary = salaires

wealth = Club { $wealth }
wealth-poor = pauvre
wealth-modest = modeste
wealth-rich = riche

## Marché

move-observe = Observer { $fighter }
move-sign = Recruter { $fighter }
move-release = Libérer { $fighter }
move-list = Mettre en vente { $fighter }
move-unlist = Retirer { $fighter } de la vente
move-keep = Rendre { $fighter } invendable
move-unkeep = Rendre { $fighter } vendable
move-accept-offer = Vendre { $fighter } à { $club } pour { $price }
move-offer = Offrir { $price } pour { $fighter }

market-signed = Recrute { $fighter }
market-released = Libère { $fighter }
market-sold = Vend { $fighter } à { $club } pour { $price }
market-bought = Achète { $fighter } à { $club } pour { $price }
market-countered = { $club } demande { $price } pour { $fighter } (offre possible cette semaine)
market-club-refused = { $club } refuse l'offre pour { $fighter }
market-fighter-refused = { $fighter } refuse de rejoindre le club
market-failed = Transfert { club-de }{ $fighter } impossible : pas de place, pas assez d'argent ou combattant parti

unsellable = Arrivé récemment : vendable à partir de la { $date }

## Personnel

role = { $role ->
        [trainer] entraîneur
        [doctor] médecin
       *[recruiter] recruteur
    }
role-title = { $role ->
        [trainer] Entraîneur
        [doctor] Médecin
       *[recruiter] Recruteur
    }

staff = Personnel : { $posts }
staff-none = Personnel : aucun
staff-post = { role } de niveau { $level }
staff-move-hire = Engager un { role } de niveau { $level }
staff-move-hire-gone = Engager un candidat parti
staff-move-dismiss = Renvoyer { $role ->
        [trainer] l'entraîneur
        [doctor] le médecin
       *[recruiter] le recruteur
    }
staff-news-hired = { role-title } de niveau { $level } engagé
staff-news-dismissed = { role-title } renvoyé

weeks-left = { $n ->
        [0] dernière semaine
        [1] dernière semaine
       *[other] encore { $n } semaines
    }


## Monde

continent-europe = Europe
continent-asia = Asie
continent-americas = Amériques
continent-africa = Afrique
continent-oceania = Océanie

culture = culture : { $culture }

profile-cautious = prudent
profile-aggressive = agressif
profile-merchant = marchand
profile-mentor = formateur
profile-veteran = vétéran
profile-showman = spectacle

arena-sand = sable
arena-pillars = piliers
arena-narrow = étroite
arena-sand-effect = le sable ralentit tout le monde, les tireurs en profitent
arena-pillars-effect = les piliers arrêtent les tirs et barrent le passage
arena-narrow-effect = le corps à corps s'engage vite

challenger = { $fighter } à { $place }

headline-title-taken = { $variant ->
        [0] { $challenger } détrône { $champion } à { $place } !
        [1] Coup de tonnerre à { $place } : { $champion } tombe devant { $challenger }
       *[other] { $place } acclame son nouveau champion, { $challenger }, vainqueur de { $champion }
    }
headline-title-kept = { $variant ->
        [0] { $champion } garde son titre à { $place } face à { $challenger }
        [1] À { $place }, { $challenger } se brise sur { $champion }
       *[other] { $champion } règne toujours sur { $place } : { $challenger } repart battu
    }
headline-title-draw = À { $place }, ni vainqueur ni vaincu : { $champion } garde son titre face à { $challenger }
headline-title-stripped = { $former } déchu à { $place } faute d'avoir défendu son titre : { $heir }
headline-heir = la couronne revient à { $holder }
headline-heir-empty = le trône reste vide
headline-title-vacated = Le champion de { $place } n'est plus : la couronne revient à { $holder }
headline-title-vacated-empty = Le champion de { $place } n'est plus, son trône reste vide
byline-versus = { $champion } contre { $challenger }
byline-heir = Nouveau champion : { $holder }

headline-champion-bought = { $club } s'offre { $fighter }, champion de { $place }, pour { $price }
headline-closed = Faillite : { $club } ferme ses portes, ses combattants partent au marché
headline-rivalry = { $club } se déclare votre rival : vos combats contre lui attireront les foules
headline-rivalry-ended = { $club } enterre la hache de guerre et met fin à sa rivalité avec vous
headline-founded = Un nouveau ludus ouvre ses portes : { $club } ({ $culture }), en { $continent }

## Journal

paper-name = Acta Diurna
paper-issue = { $date }, { $year } · les nouvelles des arènes du monde
paper-first-issue = Les nouvelles des arènes du monde
paper-no-issue = Pas encore de numéro : il paraîtra à la fin de la première semaine.
paper-older = « Numéro précédent
paper-newer = Numéro suivant »
paper-latest = Dernier numéro
paper-club = Dans nos murs
paper-titles = Les combats pour le titre
paper-defended = Titres défendus
paper-world = Échos du monde

## Relations

standing-ally = allié
standing-friend = ami
standing-neutral = neutre
standing-tense = tendu
standing-rival = rival

relation-change = { $points } : { $why } ({ $date })
reason-killed = { $fighter } a tué { $rival }
reason-beaten = { $fighter } a battu { $rival }
reason-title-taken = { $fighter } lui a pris le titre de { $place }
reason-sold = vous lui avez vendu { $fighter }
reason-bought = vous lui avez acheté { $fighter }
reason-pact-kept = { $fighter } a tenu son pacte avec { $rival } en mêlée
reason-betrayed = { $fighter } a trahi son pacte avec { $rival } en mêlée

## Commun

labelled = { $label } : { $text }
this-week = Cette semaine : { $what }
confirm = Confirmer
cancel = Annuler
back = Retour
back-to-list = Retour à la liste
potential = Potentiel
personality-title = Personnalité
record-title = Bilan
estimated-value = Valeur estimée
estimated-value-of = Valeur estimée ≈ { $value }
estimate = ≈ { $value }
negotiate = Négocier
not-ranked = non classé

## Écran du club

save-resumed = Partie reprise
save-autosave-unreadable = Sauvegarde automatique illisible : { $error }
save-saved = Partie sauvegardée
save-failed = Sauvegarde impossible : { $error }
save-loaded = Partie chargée
save-none = Aucune sauvegarde
save-unreadable = Sauvegarde illisible : { $error }
save-slot = Emplacement { $slot } : { $game }
save-slot-empty = vide
save-slot-unreadable = illisible
save-slot-game = { $club }, { $date }, club { $wealth } en { $continent }

club-becomes = Le club devient { $wealth }

top-date = Saison { $season }, semaine { $week } : { $phase ->
        [competition] compétition
        [great-event] grand événement
       *[off-season] intersaison
    }
top-money = Argent : { $money }
view-menu = Menu
view-roster = Combattants
view-fights = Résumé des combats
view-paper = Acta Diurna
view-colosseums = Colisées
view-market = Marché
view-staff = Personnel

menu-title = Menu général
menu-club = { $club } : { $wealth }, en { $continent }
menu-alerts = Alertes
menu-active = { $active } / { $max } combattants en activité
menu-next-week = Semaine suivante (Espace)
menu-read-paper = Lire l'Acta Diurna
calendar-season = Saison { $season } · semaine { $week } sur { $weeks }
phase-name = { $phase ->
        [competition] Compétition
        [great-event] Grand événement
       *[off-season] Intersaison
    }
week-title = Semaine { $week }
menu-save = Sauvegarder
menu-load = Charger
menu-new-club = club { $wealth }
menu-draw-name = Tirer un nom
menu-new-game-started = Nouvelle partie, { $club }, club { $wealth } en { $continent }, graine { $seed }
confirm-new-game = Commencer une nouvelle partie avec { $club }, club { $wealth } en { $continent } ? La partie à continuer sera perdue si elle n'est pas sauvegardée.
confirm-load = Charger l'emplacement { $slot } ({ $game }) ? La partie à continuer sera perdue si elle n'est pas sauvegardée.
confirm-overwrite = Écraser l'emplacement { $slot } ({ $game }) ?
menu-language = Langue :

pending-market = Marché : { $what }
pending-staff = Personnel : { $what }
pending-counter-offer = Marché : { $club } demande { $price } pour { $fighter }

alert-debts = Dettes : { $money }
alert-injury = { $fighter } : blessure, { $injury }
alert-tired = { $fighter } : fatigué (forme { $form })
alert-title-defense = { $fighter } : titre de { $place } à défendre au plus tard en semaine { $week }
alert-sale-offer = { $club } veut acheter { $fighter } pour { $price } (à accepter au marché cette semaine)
alert-rival = { $club } est votre rival depuis { $since } : recettes à { $receipts } %, renommée +{ $fame } par combat
alert-no-fighter = Plus aucun combattant : recrutez un free agent au marché

roster-fighter = Combattant
roster-age = Âge
roster-form = Forme
roster-morale = Moral
roster-fame = Renommée
roster-condition = État
roster-intro = Le nom d'un combattant ouvre sa fiche, où se prépare sa semaine.
roster-age-years = { $age } ans
roster-fit = En forme
roster-fame-chip = Renommée { $fame }
roster-fame-hint = Ce que le public sait de lui : elle monte avec les victoires et le spectacle, et remplit les gradins.
roster-recent = Arrivé récemment
roster-loadout = { $loadout } · aisance { $ease } %
roster-ease-hint = L'équipement choisi pour la semaine et l'aisance du combattant avec lui, qui monte à chaque combat.
roster-former = Anciens combattants ({ $count })
roster-care = avec soins
roster-no-armour = sans armure

sheet-this-week = Cette semaine
sheet-career-over = Carrière terminée
sheet-history = Historique
sheet-title = { $fighter }, { $age } ans
sheet-right-handed = droitier
sheet-left-handed = gaucher
sheet-ambidextrous = ambidextre
sheet-loadout = Configuration du combattant
sheet-origin = Origine : { $place }
origin-place = { $culture } ({ $continent })
sheet-condition = État
sheet-career = Carrière
sheet-season = Saison
sheet-stats = Caractéristiques
sheet-ease-title = Aisance
sheet-health = Santé
sheet-healthy = En bonne santé
sheet-stat-hover = Entraîné { $trained }, en combat { $fighting }
ease-loadout = { $loadout } { $ease }
ease-title = Aisance
sheet-retired-maimed = A pris sa retraite, trop mutilé
sheet-retired = A pris sa retraite
sheet-dead = Mort au combat
sheet-sold = Vendu
sheet-released = Libéré

summary-at = à { $place }
summary-time = { $seconds } s
summary-seed = Graine { $seed }
summary-money-chip = Argent { $money }
summary-fame-chip = Renommée { $fame }
summary-yours = Votre combattant
summary-opponent = L'adversaire ({ $class })
summary-damage-dealt = Dégâts infligés
summary-damage-taken = Dégâts reçus
summary-spells-cast = Sorts lancés
summary-misses = Coups ratés
summary-stuns = Étourdi
summary-hp-left = PV restants
summary-watch-again = Revoir le combat

plan-abroad = À l'étranger (voyage { $travel })
plan-care = Soigner ({ $cost })
plan-care-unaffordable = Soigner ({ $cost }) : pas assez d'argent
plan-loadout = Configuration :
plan-no-armour = Sans armure
plan-no-armour-effect = Défenses très réduites, renommée +{ $fame }
plan-fight-ill-advised = Combat déconseillé
plan-challenge = Défier :
plan-challenge-rival = { $name }, Elo { $elo }{ $accepts ->
        [false] {" "}: refuse
       *[true] {""}
    }

## Colisées et clubs rivaux

continent-colosseums = Colisées ({ $count })
continent-clubs = Clubs rivaux ({ $count })
colosseums-none = Aucun colisée de ce niveau sur ce continent pour l'instant
continent-home = { $continent } (chez vous)
clubs-culture = Culture
clubs-profile = Profil
clubs-wealth = Richesse
clubs-fighters = Combattants
clubs-relation = Relation
clubs-titles = Titres
champion-of = Champion de { $place } : { $fighter }
standing-ranked = { $place }, Elo { $elo }
standing-champion = champion, { $standing }
prestige-all = Tous
prestige-major = Majeurs
prestige-t1 = T1
prestige-t2 = T2

rival-closed = Fermé, ruiné : { $date }
rival-description = Club { $profile } et { $wealth }, { $culture }, en { $continent }, { $fighters } combattants en activité (au plus { $max })
rival-relation = Relation avec { $club } : { $relation }
rival-history = Ce qui s'est passé entre vous
rival-gone = { $gone } anciens combattants : retraités, morts, vendus ou libérés

colosseum-arena-chip = Arène : { $arena }
colosseum-prestige = Prestige { $prestige } / 5
colosseum-fee-chip = Inscription { $fee }
colosseum-fee-travel = Voyage compris
colosseum-prize-chip = Victoire { $prize }
colosseum-bonus-chip = Bonus champion { $bonus }
colosseum-champion = Champion :
colosseum-no-champion = aucun
colosseum-defend-by = à défendre d'ici la semaine { $deadline }
colosseum-title-since = Titre défendu ou pris en semaine { $since }
ranked-unnamed = n° { $number }
ranking-rank = Rang
ranking-club = Club
ranking-elo = Elo
ranking-no-club = sans club
recap-title = Classement Elo à { $place } après la semaine
recap-week = Semaine
recap-new = nouveau
colosseum-honours = Palmarès ({ $reigns } { $reigns ->
        [one] règne
       *[other] règnes
    })
reign = { $club }, de { $from } à { $until }, { $defenses ->
        [0] aucune défense
        [one] { $defenses } défense
       *[other] { $defenses } défenses
    }{ $end ->
        [beaten] , battu
        [stripped] , déchu faute de défense
        [died] , mort avec le titre
        [retired] , retiré avec le titre
        [vacated] , parti avec le titre
       *[current] {""}
    }
reign-for = pour { $club }
reign-no-club = sans club
reign-now = aujourd'hui

## Marché (vue)

market-roster = Effectif { $active } / { $max }
market-rules = Une observation et une offre par semaine ; le club rival répond à une offre en fin de semaine, puis le combattant. Un nom ouvre la fiche du combattant.
market-this-week = Cette semaine
market-tab-free = Free agents ({ $count })
market-tab-rivals = Clubs rivaux ({ $count })
market-tab-yours = Vos combattants ({ $count })
market-full = Effectif complet
market-sale-status = Vente
market-actions = Décisions de la semaine
market-listed = En vente
market-counter-offer = { $club } demande { $price } pour { $fighter }
market-offer-this-price = Offrir ce prix
market-offer-for = Offre pour { $fighter } :
market-rival-refuses = { $club } est votre rival : il refuse de traiter avec vous
market-make-offer = Faire l'offre
market-free-agents = Gratuits à recruter ; ils combattent dans les colisées sans droit d'entrée.
filter-age = Âge de
filter-to = à
filter-fame = Renommée min.
filter-wins = Victoires min.
filter-win-rate = Taux de victoires min.
filter-clear = Tout montrer
outside-club = Club : { $club }
outside-summary = { $condition }, renommée { $fame }, bilan { $record }
never-observed = Jamais observé
observed = Observé { $weeks }
observe = Observer
observe-again = Observer (déjà { $weeks } sem.)
sign = Recruter
unlist = Retirer de la vente
unkeep = Rendre vendable
keep = Invendable
list = Mettre en vente
accept-offer = Accepter { $club } : { $price }
release = Libérer

## Personnel (vue)

staff-intro = L'entraîneur rend l'entraînement plus profitable et moins fatigant, le médecin rend les soins plus efficaces et évite des séquelles, le recruteur montre mieux les combattants observés. Les candidats arrivent et partent du marché, chacun avec ses stats et son salaire, et les clubs rivaux y engagent aussi. Chaque salaire est payé chaque semaine, même à crédit ; un changement compte dès la semaine qui se joue.
staff-candidates = Candidats
staff-tab-held = { $role } (niveau { $level })
staff-salary = Salaire { $salary } / sem.
staff-session = Séance { $session }
staff-session-hint = Prix de chaque séance d'entraînement
staff-progress = Progression +{ $progress } %
staff-form = Forme épargnée { $form }
staff-care-weeks = Convalescence { $kept } %
staff-care-weeks-hint = Part des semaines d'arrêt qui restent après les soins
staff-care-cost = Prix des soins { $cost } %
staff-spared = Séquelles évitées { $spared } %
staff-revealed = +{ $revealed } stat / sem.
staff-ranges = Fourchettes { $width } %
staff-ranges-hint = Largeur des fourchettes de stats d'un combattant observé, par rapport à l'habituelle
staff-potential = { $potential ->
        [0] Potentiel non estimé
       *[other] Potentiel ± { $potential } %
    }
staff-personality = { $seen ->
        [true] Personnalité vue
       *[false] Personnalité cachée
    }
staff-ease = { $seen ->
        [true] Aisance vue
       *[false] Aisance cachée
    }
staff-nobody = Aucun
staff-no-candidate = Aucun candidat sur le marché
staff-level = Niveau { $level }
dismiss = Renvoyer
hire = Engager

## Écran de combat

controls = Contrôles
speed-slow = x0,5
speed-normal = x1
speed-fast = x1,5
skip-to-result = Passer au résultat
continue = Continuer
pause = Pause
resume = Reprendre
leave-fight = Quitter le combat
timer = Chrono
elimination = Élimination
gauges = Jauges
mana = Mana
endurance = Endurance
elements = Éléments
spell-ready = prêt
spell-recharging = { $seconds } s
spell-too-dear = trop cher
spell-auto-attack = Attaque de base : seulement quand aucune autre capacité n'est payable ni utile
cost-mana = { $amount } mana
cost-endurance = { $amount } end.
cost-elements = { $amount } élém.
options = Options
options-graphics = Graphismes
options-audio = Audio
options-language = Langue
close = Fermer

## Menu principal

title-continue = Continuer
title-load = Charger une partie
title-new-game = Nouvelle partie
title-quit = Quitter
new-continent = Continent
new-culture = Culture
new-name = Nom du club
new-wealth = Commencer avec un :
game = Partie
main-menu = Menu principal
fullscreen = Plein écran
interface-size = Taille de l'interface :
music = Musique
effects = Effets

fighter-number = combattant { $number }
result-time-up = Temps écoulé : personne ne remporte le combat.
result-winner = { $winner } remporte le combat.
result-opponent = { $name }, l'adversaire,
result-no-winner = Personne ne remporte le combat.
result-out = Hors du combat : { $ways }.
verdict-victory = Victoire !
verdict-defeat = Défaite
verdict-no-winner = Pas de vainqueur
out-dead = { $who } est tué.
out-gave-up = { $who } abandonne.
out-eliminated = { $who } est éliminé faute de coups.
out-knocked-out = { $who } est mis K.-O.
out-left-for-dead = { $who } tombe, laissé pour mort, mais survit.
out-beheaded = { $who } est décapité.
out-severed = { $who } a un membre tranché.
out-count-dead = { $count ->
        [one] 1 tué
       *[other] { $count } tués
    }
out-count-gave-up = { $count ->
        [one] 1 abandon
       *[other] { $count } abandons
    }
out-count-eliminated = { $count ->
        [one] 1 éliminé faute de coups
       *[other] { $count } éliminés faute de coups
    }
out-count-knocked-out = { $count } K.-O.
out-count-left-for-dead = { $count ->
        [one] 1 laissé pour mort
       *[other] { $count } laissés pour morts
    }
out-count-beheaded = { $count ->
        [one] 1 décapité
       *[other] { $count } décapités
    }
out-count-severed = { $count ->
        [one] 1 membre tranché
       *[other] { $count } membres tranchés
    }
feed = Déroulé
feed-hit = { $caster } : { $spell } sur { $target }, -{ $damage } PV
feed-hit-part = { $caster } : { $spell } sur { $target }, -{ $damage } PV ({ $part })
feed-miss = { $caster } : { $spell } sur { $target }, raté
feed-blast = Une mine frappe { $target }, -{ $damage } PV
feed-bleeding = { $who } saigne ({ $part }).
feed-blinded = { $who } est aveuglé : -{ $percent } % de précision.
feed-stunned = { $who } est étourdi.
feed-rise = { $who } se relève.
feed-severed = { $who } perd un membre ({ $part }).
feed-pact = { $a } et { $b } s'allient.
feed-betrayal = { $betrayer } trahit son allié.
feed-pact-ended = L'alliance entre { $a } et { $b } prend fin.
popup-damage = -{ $damage }
popup-miss = raté
popup-bleeding = saigne
popup-blinded = aveuglé
popup-stunned = étourdi
popup-knocked-out = K.-O.
popup-gave-up = abandonne
popup-severed = tranché !
popup-beheaded = décapité !
state-stunned = Étourdi, { $seconds } s
state-bleeding = Saigne : { $part }, { $seconds } s
state-blinded = Aveuglé : -{ $percent } % de précision, { $seconds } s
final-duel = Duel final
elimination-next = Élimination dans { $seconds } s : { $out ->
        [1] 1 combattant sort
       *[other] { $out } combattants sortent
    }
elimination-threshold = Pour rester, plus de { $threshold } dégâts depuis la dernière élimination
elimination-standing = { $fighter } : { $damage } dégâts, { $danger ->
        [true] éliminé si rien ne change
       *[false] à l'abri
    }

## Données de conception

class-mirmillon = Mirmillon
class-augure = Augure
class-gladiateur = Gladiateur
class-maitre-terrain = Maître du terrain

loadout-glaive-shield = Glaive et bouclier
loadout-two-glaives = Double glaive
loadout-staff = Bâton
loadout-staff-shield = Bâton et bouclier
loadout-sword-shield = Épée et bouclier
loadout-two-swords = Double épée
loadout-dagger-shield = Dague et bouclier
loadout-two-daggers = Double dague

colosseum-capoue = Capoue
colosseum-rome = Rome
colosseum-pompei = Pompéi
colosseum-samarcande = Samarcande
colosseum-chang_an = Chang'an
colosseum-karakorum = Karakorum
colosseum-cahokia = Cahokia
colosseum-cuzco = Cuzco
colosseum-tenochtitlan = Tenochtitlan
colosseum-mbanza_kongo = Mbanza Kongo
colosseum-musumba = Musumba
colosseum-nan_madol = Nan Madol
colosseum-ngapuhi = Ngāpuhi

culture-rome = Rome
culture-grece = Grèce
culture-celtes = Celtes
culture-germains = Germains
culture-steppe = Steppe
culture-asie_est = Asie de l'Est
culture-inde = Inde
culture-asie_ouest = Asie de l'Ouest
culture-arabe = Monde arabe
culture-afrique = Tribus d'Afrique
culture-amerindiens = Tribus amérindiennes
culture-mesoamerique = Mésoamérique
culture-amerique_sud = Tribus d'Amérique du Sud
culture-oceanie = Tribus d'Océanie

# « de » devant un nom de club ou de combattant, élidé devant une voyelle ($vowel).
club-de = { $vowel ->
        [true] d'
       *[false] {"de "}
    }
club-ludus = Ludus { $name }
club-familia = Familia { $name }
club-domus = Domus { $name }
club-palestre = Palestre { club-de }{ $name }
club-gymnase = Gymnase { club-de }{ $name }
club-clan = Clan { club-de }{ $name }
club-tribu = Tribu { club-de }{ $name }
club-halle = Halle { club-de }{ $name }
club-bande = Bande { club-de }{ $name }
club-ordu = Ordu { club-de }{ $name }
club-horde = Horde { club-de }{ $name }
club-dojo = Dōjō { $name }
club-maison_asie = Maison { $name }
club-ecole = École { $name }
club-akhara = Akhara { club-de }{ $name }
club-ashram = Ashram { club-de }{ $name }
club-zourkhaneh = Zourkhaneh { club-de }{ $name }
club-maison = Maison { club-de }{ $name }
club-dar = Dar { $name }
club-bayt = Bayt { $name }
club-kraal = Kraal { club-de }{ $name }
club-lignee = Lignée { club-de }{ $name }
club-loge = Loge { club-de }{ $name }
club-cercle = Cercle { club-de }{ $name }
club-maison_aigle = Maison de l'Aigle { club-de }{ $name }
club-maison_jaguar = Maison du Jaguar { club-de }{ $name }
club-calmecac = Calmecac { club-de }{ $name }
club-ayllu = Ayllu { club-de }{ $name }
club-whare = Whare { $name }
club-pa = Pā { club-de }{ $name }
club-numbered = { $club } { $number }
