# Decompilation locale

Meme methode que Horizon dans Rusted : la copie de Terraria du joueur sert de
reference de comportement, lue en local, jamais publiee.

## Produire les sources

```sh
dotnet tool install -g ilspycmd --version 8.2.0.7535
tools/decompile.sh "/chemin/vers/Terraria"
```

Le script ecrit `.local/terraria/<empreinte de Terraria.exe>/sources/` et le
journal d'ILSpy a cote. `.local/` est ignore par git.

## Regles

- Aucune source decompilee, aucun binaire (`.exe`, `.dll`) ni asset (`.xnb`,
  `.wld`, banques XACT) dans le depot ou dans un commit.
  `tools/check_public_tree.py` le verifie sur l'arbre et sur tout l'historique,
  en CI et en local.
- Les sources sont une aide a la lecture : on en tire les regles (constantes,
  ordre des operations, tables de raccords), puis on les reecrit en Rust avec
  nos propres structures. Pas de copie ni de traduction ligne a ligne, pas de
  commentaires recopies.
- Quand c'est faisable, un test compare notre resultat a celui du jeu sur les
  vrais fichiers du joueur (tests ignores sans `TERRARIA_PATH`).
