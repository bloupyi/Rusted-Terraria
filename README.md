# Rusted Terraria

Moteur Rust qui reimplemente Terraria from scratch, en visant des performances
nettement superieures a l'original. Phase 1 : charger un monde `.wld` existant et
l'afficher en lecture seule, camera libre.

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

Controles : ZQSD/WASD ou fleches pour se deplacer (Maj pour accelerer), molette pour
zoomer autour du curseur, clic gauche ou milieu pour glisser, Echap pour quitter.
Le titre affiche FPS, temps de frame, nombre de draws et d'instances.

## Organisation

| Crate | Role |
|---|---|
| `crates/formats` | Formats du jeu : XNB (LZX, LZ4, Texture2D), WLD, recherche de l'installation. Aucune dependance graphique |
| `crates/world` | Monde en chunks 32x32, struct of arrays |
| `crates/render` | Atlas de sheets, instances par chunk construites avec rayon, rendu wgpu |
| `crates/app` | Binaire `rusted-terraria` : fenetre winit, boucle, camera |

Choix de rendu :

- Chaque sheet de tuiles ou de murs est une region d'un texture array 4096x4096 :
  un seul bind group, donc les chunks contigus d'une meme ligne se dessinent en un
  seul draw instancie (murs puis tuiles).
- Instance de 8 octets : position de tuile, coordonnees atlas, couche.
- Sous 2 pixels par tuile, une carte d'un texel par tuile (couleur moyenne de la
  sheet) remplace les instances : un draw quel que soit le zoom arriere.

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

## Limites connues (phase 1)

Marquees `TODO` dans le code :

- WLD : versions 225 a 279 verifiees par la documentation ; les versions plus
  recentes sont lues avec la meme disposition et refusees si la section des tuiles
  ne tombe pas juste. Shimmer (bit 7 de l'en-tete 3) non verifie sur une vraie sauvegarde.
- XNB : LZ4 suppose un bloc brut sans en-tete de frame (non verifie, Terraria
  utilise LZX ou rien). Formats DXT non decodes.
- Cadrage des blocs approximatif (voisins identiques seulement), pas de fusion
  terre/pierre, herbe, pentes, peinture, liquides, arbres ni eclairage.
