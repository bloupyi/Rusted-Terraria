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
- Clic gauche : creuser (un objet entier s'il est multi-tuiles) ; les blocs tombent
  en objets que le joueur ramasse en passant pres. Clic droit : utiliser l'objet
  choisi (poser un bloc, vider ou remplir un seau). Portee de 10 tuiles.
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
| `crates/formats` | Formats du jeu : XNB (LZX, LZ4, Texture2D), WLD, recherche de l'installation. Aucune dependance graphique |
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
liquides. Dessine en rectangles de couleur faute de sprite.

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

## Limites connues

Marquees `TODO` dans le code :

- WLD : versions 225 a 279 verifiees par la documentation ; les versions plus
  recentes sont lues avec la meme disposition et refusees si la section des tuiles
  ne tombe pas juste. Shimmer (bit 7 de l'en-tete 3) non verifie sur une vraie sauvegarde.
- XNB : LZ4 suppose un bloc brut sans en-tete de frame (non verifie, Terraria
  utilise LZX ou rien). Formats DXT non decodes.
- Cadrage des blocs approximatif (voisins identiques seulement), pas de fusion
  terre/pierre, herbe, pentes, peinture ni arbres.
- Les objets multi-tuiles (meubles, torches) disparaissent sans objet a ramasser.
- Liquides, joueur et objets sans textures du jeu (couleurs unies) : les noms et la
  disposition des sheets `Liquid_*` et `Player_*` ne sont pas verifies sans
  installation.
