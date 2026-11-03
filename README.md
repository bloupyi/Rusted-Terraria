# Rusted Terraria

Moteur Rust qui reimplemente Terraria from scratch, en visant des performances
nettement superieures a l'original. Phase 1 : charger un monde `.wld` existant et
l'afficher en lecture seule, camera libre. Phase 2 : eclairage avec cycle jour/nuit,
liquides qui s'ecoulent, joueur avec physique, creuser et poser des blocs.

## Contraintes

- Aucun asset ni code de Re-Logic dans ce depot (`.xnb`, `.wld`, `.plr`, textures,
  sons, code decompile). `.gitignore` et la CI les refusent.
- Les textures sont lues au lancement depuis l'installation Steam du joueur.
- Approche clean-room : comportement reimplemente a partir de documentation
  publique et d'observation, jamais par traduction de code decompile.

## Lancer

```powershell
cargo run --release -- "$env:USERPROFILE\Documents\My Games\Terraria\Worlds\MonMonde.wld"
```

Dossier Terraria, dans l'ordre :

1. variable `TERRARIA_PATH` ;
2. `terraria_path = D:\Jeux\Terraria` dans `rusted-terraria.cfg` (dossier courant
   ou a cote de l'executable) ;
3. `C:\Program Files (x86)\Steam\steamapps\common\Terraria`.

Sans installation trouvee, les tuiles sont affichees avec des couleurs de
remplacement. `--vsync` active la synchro verticale (desactivee par defaut pour
mesurer les FPS).

Controles :

- ZQSD/WASD ou fleches : marcher, sauter (haut ou Espace), descendre des plateformes.
- Clic gauche : creuser. Un meuble ou une torche part en entier et donne un objet
  qui le repose a l'identique ; un arbre tombe en entier et donne du bois ; ce qui
  reposait sur le bloc casse tombe aussi. Les objets ramasses vont dans
  l'inventaire en passant pres. Clic droit : utiliser l'objet choisi (poser un bloc
  ou un objet, vider ou remplir un seau). Portee de 10 tuiles.
- 1 a 0 ou Q/E : choisir l'emplacement de la barre d'objets (en haut a gauche ;
  la barre blanche sous un objet donne la taille de la pile). Depart : seaux d'eau,
  de lave, de miel, de shimmer et un seau vide.
- F : camera libre (deplacement au clavier, Maj pour accelerer, clic milieu pour
  glisser, edition sans limite de portee ni consommation d'objets) ; `--free` pour
  demarrer ainsi.
- Molette : zoom autour du curseur. L : couper l'eclairage. T (maintenu) : accelerer
  le temps. Echap : quitter.

Le titre affiche FPS, temps de frame, draws, instances, heure, outil et nombre de
chunks de liquide actifs.

## Organisation

| Crate | Role |
|---|---|
| `crates/formats` | Formats du jeu : XNB (LZX, Texture2D), WLD, recherche de l'installation. Aucune dependance graphique |
| `crates/world` | Monde en chunks 32x32, struct of arrays, editions, lumiere, liquides, physique du joueur |
| `crates/render` | Atlas de sheets, instances par chunk construites avec rayon, rendu wgpu |
| `crates/app` | Binaire `rusted-terraria` : fenetre winit, boucle, camera, jeu (`game.rs`), inventaire |

Choix de rendu :

- Chaque sheet de tuiles ou de murs est une region d'un texture array 4096x4096 :
  un seul bind group, donc les chunks contigus d'une meme ligne se dessinent en un
  seul draw instancie (murs puis tuiles).
- Instance de 8 octets : position de tuile, coordonnees atlas, couche.
- Sous 2 pixels par tuile, une carte d'un texel par tuile (couleur moyenne de la
  sheet) remplace les instances : un draw quel que soit le zoom arriere.

Eclairage :

- Une valeur RGBA par tuile (`world::light`) : RGB pour les tuiles lumineuses (24
  styles de torches, lampes, champignons, pierre infernale...) et les liquides
  (lave, miel, shimmer), alpha pour le ciel ouvert (chaque colonne jusqu'au premier
  bloc ou mur). Le shader multiplie l'alpha par la couleur du ciel selon l'heure :
  le cycle jour/nuit (15 min de jour, 9 de nuit) ne recalcule rien.
- Propagation par balayages droite, gauche, bas, haut, repetes deux fois ; la
  lumiere qui sort d'une case est attenuee selon la case (air, liquide, bloc).
  Lignes et bandes de colonnes en parallele, environ 300 ms sur 8400x2400 et 4 coeurs.
- Envoyee au GPU dans des pages 2048x2048 comme la carte ; le shader interpole
  entre les centres des tuiles.
- Apres une edition, seule une fenetre autour des chunks modifies est recalculee
  (chunks + 50 tuiles, au-dela toute source est eteinte) : meme resultat qu'un
  calcul complet. Les chunks visibles d'abord, 64 par image au plus.

Liquides (`world::liquid`) :

- Automate par cellule sur les chunks actifs, 30 fois par seconde : chute, puis
  nivellement avec les voisins une fois pose. Eau et lave donnent de l'obsidienne.
  Un chunk ou rien ne bouge s'endort jusqu'a ce qu'un voisin change.
- Rendu en quads de couleur entre murs et tuiles, hauteur selon la quantite, plein
  si du liquide est au-dessus, surface ondulee. Couleur opaque sur la carte.

Monde modifiable :

- Les editions marquent leurs chunks (et les voisins en bordure, le cadrage des
  blocs lisant les voisins). Chaque ligne de chunks a deux buffers : murs et tuiles,
  qui ne changent qu'avec les blocs, et liquides, reconstruits a chaque mouvement.
- Chunks visibles remailles dans l'image, les autres 64 par image ; carte 256
  chunks par image.

Joueur (`world::player`) : boite de 20x42 px deplacee axe par axe contre les blocs,
sous-pas de moins d'une demi-tuile, marches d'une tuile montees seules,
plateformes traversables par dessous ou en descendant, nage ralentie dans les
liquides. Dessine avec les sheets `Player_0_*` et `Player_Hair_1` teintees, jambes
animees en marche.

Monstres (`world::mob`) : corps partage avec le joueur (`world::body`). Slimes le
jour (attente puis saut vers le joueur), zombies en plus la nuit (marche, saut
devant un mur de deux blocs). Ils apparaissent hors ecran a 35-55 tuiles, au plus
5 a la fois, et disparaissent au-dela de 100 tuiles. Contact : degats, recul et
invulnerabilite courte ; a 0 PV le joueur revient au point de depart. Clic gauche
sur un monstre a portee : 10 degats avec recul. Barre de vie sous la barre d'objets.

Son (`app/src/audio.rs`) : effets `Content/Sounds/*.xnb` (SoundEffect PCM 16 bits,
les 527 fichiers de 1.4.5 sont lus) charges a la demande, reechantillonnes a la
frequence de sortie et mixes dans le callback cpal. Creuser, poser, seaux,
ramassage, coups, monstres touches ou tues, gemissements de zombies. Sans sortie
audio ou sans installation, le jeu reste muet. La musique (`Wave Bank.xwb`, XACT)
n'est pas lue.

Multijoueur (`app/src/net.rs`, protocole maison sur TCP, port 7777 par defaut) :
`rusted-terraria --host monde.wld` (ou `--port <port>`) heberge, et
`rusted-terraria --join <adresse[:port]>` rejoint sans fichier monde : l'hote envoie
le monde en `.wld` a la connexion. Ensuite joueurs, editions de tuiles et de
liquides, monstres (simules par l'hote) et heure du jour sont synchronises ; les
coups d'un client sont appliques par l'hote. Les liquides s'ecoulent de chaque cote
sans synchronisation fine, et les objets au sol restent locaux.

Sprites : joueur, monstres (`NPC_1`, `NPC_3`), objets au sol, barre d'objets (icones tirees des sheets de tuiles
et `Item_*` pour les seaux), cimes et branches d'arbres (`Tree_Tops_0`,
`Tree_Branches_0`) et fond souterrain sont des rectangles textures du meme atlas,
eclaires comme les tuiles. Les liquides utilisent `Liquid_0` (eau animee),
`Liquid_1` et `Liquid_11` ; le shimmer reste en couleur unie.

## Tests

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Les tests utilisent des fichiers synthetiques ecrits par le code. Les tests sur de
vrais fichiers sont ignores sans variable : `TERRARIA_PATH` (lit
`Content\Images\Tiles_0.xnb`) et `RUSTED_TEST_WLD` (chemin d'un monde).

Monde synthetique de 8400x2400 pour les mesures :

```powershell
cargo run --release -p world --example gen_world -- big.wld
cargo run --release -p world --example load_world -- big.wld
```

## Verifie sur les vrais fichiers

Terraria 1.4.5 et un vrai monde (6400x1800, version 326) :

- XNB : 13 993 fichiers en LZX, 22 non compresses, aucun en LZ4 ni en DXT ; tous
  lus. Le chemin LZ4 a ete retire.
- WLD : version 326 lue avec la meme disposition ; bit shimmer confirme.
- Chargement : lecture 0.25 s, scene 0.4 s, envoi GPU 0.5 a 1.2 s.
- 40 a 47 FPS en 800x600 avec le rendu logiciel lavapipe (pas de vrai GPU teste).

## Limites connues

Marquees `TODO` dans le code :

- WLD : versions anterieures a 225 non verifiees.
- Cadrage des blocs approximatif (voisins identiques seulement), pas de fusion
  terre/pierre, pentes ni peinture.
- Un seul style d'arbre (foret) pour les cimes ; pas de shimmer texture.
