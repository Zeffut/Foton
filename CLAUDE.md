# CLAUDE.md — Serveur Minecraft en Rust (fork privé de SteelMC)

## Identité du projet

Fork **privé** de [Steel-Foundation/SteelMC](https://github.com/Steel-Foundation/SteelMC) —
serveur Minecraft Java Edition en Rust, cible **MC 26.2**.

- **origin**   → `https://github.com/Zeffut/SteelMC.git` (privé, notre version)
- **upstream** → `https://github.com/Steel-Foundation/SteelMC.git` (amont, lecture seule)

Nous développons librement ici. L'amont reste une source d'améliorations à merger,
pas une autorité sur nos choix.

## Emplacement — IMPORTANT

Le workspace est **`/root/SteelMC` dans WSL2 (Ubuntu 26.04)**, jamais côté Windows.

- Compilation : toujours dans WSL. Smart App Control bloque les build scripts côté Windows.
- Édition depuis les outils Windows : `\\wsl.localhost\Ubuntu\root\SteelMC\...`
- Exécution : passer par un script `.sh` et `wsl -d Ubuntu -- bash /mnt/c/.../script.sh`.
  Ne jamais inliner `$HOME` dans une commande PowerShell : l'interpolation casse la syntaxe bash.

## Commandes

```bash
cargo check --workspace --all-targets   # ~85 s à froid
cargo clippy -r --all-targets --all-features
cargo fmt --all --check
cargo test --workspace
typos
prek run --all-files                    # suite pré-commit complète
```

Toolchain **nightly-2026-07-23** (pinée par `rust-toolchain.toml`), edition 2024.
Outils disponibles : `typos`, `ast-grep` (recherche/réécriture structurelle), `prek`, `gh`, Java 21.

## Règles techniques — à respecter

Héritées de l'amont parce qu'elles sont **techniquement justes**, pas par obligation.
Un serveur qui diverge de vanilla est un serveur cassé.

1. **Parité vanilla 1:1** pour tout ce qui est observable en jeu : gameplay, protocole,
   registres, worldgen. Vérifier contre la source décompilée dans `minecraft-src/`
   (générée par `./update-minecraft-src.sh`) avant d'implémenter — ne jamais coder de mémoire.
2. **Aucune donnée inventée.** Les valeurs de registres/blocs/items/worldgen viennent de
   SteelExtractor (`/root/SteelExtractor`), jamais d'une transcription manuelle.
3. **Pas de stub dans les fondations.** `todo!()`, mocks et valeurs bidon uniquement en
   prototypage explicitement identifié.
4. **Pas de `.unwrap()`/`.expect()` en production**, pas d'`unsafe` hors du `DowncastType`
   keyé, pas de lint désactivé sans `#[expect(..., reason = "...")]`.
5. **Ne jamais éditer `src/generated/`** — c'est produit au build. Modifier le `build/`
   correspondant ou les données extraites.
6. **Style** : guard clauses plutôt qu'indentation profonde, `Result` pour le récupérable,
   pas de wrapper trivial, fichiers focalisés, commentaires concis.
7. **Tests** : seulement s'ils attrapent une régression plausible. Pas de test qui redit
   une constante ou une évidence que le compilateur garantit déjà.

Référence complète : `AGENTS.md` à la racine (document de l'amont, toujours valable).

**Ce qu'on abandonne de l'amont** : leur interdiction du développement IA autonome, et
l'obligation de discuter chaque changement sur leur Discord. C'est notre repo.

## Architecture

```
steel (binaire) → steel-login (connexion) → steel-core (logique de jeu)
  → steel-worldgen → steel-protocol (paquets) → steel-macros
  → steel-registry (données générées) → steel-utils / steel-math → steel-crypto
```

| Zone | Chemin |
|------|--------|
| Logique de jeu | `steel-core/src/` |
| Behaviors blocs | `steel-core/src/behavior/blocks/` |
| Behaviors items | `steel-core/src/behavior/items/` |
| Entités | `steel-core/src/entity/entities/` |
| Block entities | `steel-core/src/block_entity/` |
| Monde / chunks | `steel-core/src/world/`, `steel-core/src/chunk/` |
| Paquets | `steel-protocol/src/packets/game/` |
| Handlers joueur | `steel-core/src/player/networking.rs` |
| Build scripts | `steel-core/build/`, `steel-registry/build/`, `steel-worldgen/build/` |
| Source vanilla | `minecraft-src/minecraft/src/net/minecraft/` |

## Mécanisme d'implémentation — le point central

`steel-core/build/classes.json` associe chaque entrée vanilla à sa classe Java :

```json
{ "name": "lantern", "class": "LanternBlock" }
```

Au build, le script scanne `src/behavior/blocks/**/*.rs` pour les structs annotées
`#[block_behavior]`, croise le nom de struct avec le champ `class`, et **génère
l'enregistrement**. Donc :

> Implémenter un bloc = créer une struct `#[block_behavior]` portant **exactement** le nom
> de la classe vanilla, dans la bonne catégorie, exportée dans le `mod.rs` voisin.
> Aucune ligne d'enregistrement à écrire.

Même principe pour `#[item_behavior]` et `#[entity_behavior]`.

Méthode de travail : ouvrir la classe Java dans `minecraft-src/`, relever les méthodes
surchargées, transposer. Le trait `BlockBehavior` documente chaque méthode par son
équivalent vanilla.

## État de la couverture (22/08/2026)

| Catégorie | Classes couvertes |
|-----------|-------------------|
| Blocs     | 185 / 265 (70 %) |
| Items     | 30 / 70 (43 %) |
| Entités   | 4 / 142 (3 %) |

Manques structurants : **aucun mob hostile** (seuls vache, cochon, mouton existent),
**pas de fourneau**, **pas de coffre**, craft limité à la table. La survie est donc
impossible en l'état — c'est là que se trouve le travail à fort impact.

## Workflow git

- **Une branche par sujet**, jamais de commit direct sur `master`
  (le hook `no-commit-to-branch` de prek le bloque de toute façon).
- Nommage : `feat/<sujet>`, `fix/<sujet>`, `refactor/<sujet>`.
- Messages en **conventional commits** : `feat: implement nylium blocks`,
  `fix(setblock): correct inverted keep mode`.
- **Commits atomiques et fréquents** : un commit = un changement cohérent qui compile.
- Vérifier avant chaque commit : `cargo check` au minimum, la suite CI avant de merger.
- Merge dans `master` une fois la CI verte.

Synchroniser l'amont régulièrement :

```bash
git fetch upstream
git merge upstream/master     # depuis master, puis résoudre les conflits
git push origin master
```

## Licence — AGPL-3.0

Le code dérive de SteelMC sous AGPL-3.0. Conséquences :

- Notre version reste sous **AGPL-3.0**, relicensing propriétaire impossible.
- Conserver les mentions de copyright et signaler nos modifications.
- **Clause réseau** : dès qu'un serveur tournant sous ce code est accessible à des tiers,
  ceux-ci ont droit au code source de notre version. Développer en privé n'entraîne
  aucune obligation ; déployer publiquement en entraîne une.
