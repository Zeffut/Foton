# CLAUDE.md — Foton, serveur Minecraft en Rust

## Identité du projet

**Foton** — serveur Minecraft Java Edition écrit en Rust, cible **MC 26.2**.

Projet souverain et autonome : nous décidons de l'architecture, du périmètre et du
rythme. Le code dérive d'une base AGPL-3.0 antérieure dont l'avis de copyright est
conservé dans `LICENSE` — c'est la seule obligation qui subsiste, et elle est
remplie.

- **origin** → `https://github.com/Zeffut/Foton.git` (privé)
  Le dépôt a été renommé le 2026-08-29 ; GitHub redirige l'ancienne URL, mais tout
  clone existant ailleurs gagne à faire
  `git remote set-url origin https://github.com/Zeffut/Foton.git`.
- Aucun remote `upstream`, et aucun script de synchronisation : le lien avec le
  dépôt d'origine est coupé.

Le renommage est terminé partout (crates, namespaces `foton:`, permissions
`foton.*`, variables `FOTON_*`, brand payload `Foton`, chaînes visibles en jeu,
tags, docs publiées, `FotonExtractor`). Deux occurrences de l'ancien nom
subsistent et sont voulues : l'item vanilla `flint_and_steel`, et l'URL de la
dépendance `TextComponents` dans `Cargo.toml`, qui pointe encore sur le dépôt
public d'origine.

## Emplacement — IMPORTANT

Deux checkouts existent, et **les deux compilent** :

- **Windows** : `C:\Users\Zeffu\Desktop\Projets\Foton`.
- **WSL2 (Ubuntu)** : `/root/Foton` — renommé depuis `/root/SteelMC` le 2026-09-03.

Sur le checkout Windows, `cargo check`, `cargo clippy` et `cargo test` tournent
**nativement** et plus vite qu'en passant par WSL. La version précédente de ce
document interdisait de compiler côté Windows ; c'était faux, et l'interdiction
a coûté du temps.

Ce que Smart App Control bloque vraiment, avec `os error 4551` :

- **`cargo fmt`**, toujours.
- **les binaires de test fraîchement liés**, par intermittence — `cargo test`
  peut passer puis se voir refuser après un rebuild, ce qui emporte aussi
  `dev/count-tests.py`.

Pour ces cas-là, et pour `typos` et `prek`, piloter WSL sur le **même** checkout
Windows, sans dupliquer l'arbre :

```
wsl -d Ubuntu -u root -- bash -c 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton \
  && export CARGO_TARGET_DIR=/root/foton-target \
  && /root/.cargo/bin/cargo fmt --all'
```

`CARGO_TARGET_DIR` est indispensable : sans lui, les artefacts Linux et MSVC se
disputent le même `target/debug`. Les outils vivent dans `/root/.cargo/bin/` et
ne sont **pas** sur le `PATH` ; les invoquer par chemin absolu.

Trois pièges d'invocation depuis Git Bash, chacun ayant produit un faux
diagnostic :

- `wsl.exe` mange les variables de shell — une boucle `for t in a b; do ... $t`
  s'exécute avec `$t` vide. Écrire des commandes sans variable, ou un script.
- un chemin `/mnt/...` passé comme argument nu devient
  `C:/Program Files/Git/mnt/...`. Le passer dans un `bash -c '...'`.
- `wsl -d Ubuntu` s'exécute en tant que `zeffu`, qui ne peut pas lire `/root`.
  Passer `-u root`, sinon les checkouts y paraissent absents.

Enfin, les scripts Python de `dev/` exigent `PYTHONUTF8=1` côté Windows : sans
lui `gen-config-docs.py` meurt sur cp1252 **après** avoir ouvert sa sortie, ce
qui vide `CONFIGURATION.md`.

## Commandes

```bash
bash dev/doctor.sh          # vérifie que l'environnement est complet et cohérent
bash dev/ci.sh              # rejoue toute la suite de vérification (~150 s)
bash dev/smoke-test.sh      # démarre le serveur et lui parle en protocole Minecraft
bash dev/join-test.sh       # fait entrer un vrai client dans le monde (login → play)
python3 dev/coverage.py     # mesure la couverture réelle par rapport à vanilla
python3 dev/gen-config-docs.py  # régénère CONFIGURATION.md depuis les schémas
python3 dev/check-natives.py    # chaque native déclarée est enregistrée, et rien d'autre
bash dev/bedrock-test.sh        # Geyser, identité Floodgate, persistance de l'UUID
```

Commandes brutes :

```bash
cargo check --workspace --all-targets   # ~85 s à froid
cargo clippy -r --all-targets --all-features
cargo fmt --all --check
cargo test --workspace
typos
prek run --all-files                    # suite pré-commit complète
```

Toolchain **nightly-2026-07-23** (pinée par `rust-toolchain.toml`), edition 2024.
Outils : `typos`, `ast-grep` (recherche/réécriture structurelle), `prek`, `gh`.

**Java — trois usages, ne pas confondre :**

| Usage | JDK | `JAVA_HOME` |
|-------|-----|-------------|
| `update-minecraft-src.sh` (GitCraft) | **25** | `/usr/lib/jvm/java-25-openjdk-amd64` |
| FotonExtractor (Fabric Loom) | **21** | `/usr/lib/jvm/java-21-openjdk-amd64` |
| `build-plugin-api.sh` | n'importe lequel ≥ 21 | — |

GitCraft échoue avec Java 21 (`release version 25 not supported`).

Le jar de l'API plugin est compilé avec `javac --release 21`, quel que soit le
JDK qui le construit : Paper 26.2 exige Java 21, et un jar produit sans cette
option par un JDK 25 porte la version de classe 69, qu'un serveur en Java 21 ne
peut pas charger — l'hôte de plugins meurt alors sans rien nommer.

## Règles techniques — à respecter

Héritées de l'amont parce qu'elles sont **techniquement justes**, pas par obligation.
Un serveur qui diverge de vanilla est un serveur cassé.

1. **Parité vanilla 1:1** pour tout ce qui est observable en jeu : gameplay, protocole,
   registres, worldgen. Vérifier contre la source décompilée dans `minecraft-src/`
   (générée par `./update-minecraft-src.sh`) avant d'implémenter — ne jamais coder de mémoire.
   Sources vanilla présentes : **1,1 Go, 7 055 fichiers Java, MC 26.2**, dans
   `minecraft-src/minecraft/src/net/minecraft/`. `dev/doctor.sh` vérifie qu'elles
   correspondent bien à la cible de `Cargo.toml`.
2. **Aucune donnée inventée.** Les valeurs de registres/blocs/items/worldgen viennent de
   FotonExtractor (`/root/FotonExtractor`), jamais d'une transcription manuelle.
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

### Carte de la documentation

| Fichier | Rôle |
|---------|------|
| `README.md` | ce qu'est Foton, comment le lancer, où il en est |
| `AGENTS.md` | les règles d'ingénierie (le standard technique) |
| `CONFIGURATION.md` | chaque clé de config — **généré**, voir plus bas |
| `PARITY.md` | le registre de parité vanilla et pourquoi les chiffres mentent |
| `CONTRIBUTING.md` | la barre qu'un changement doit passer |

`CONFIGURATION.md` est produit par `python3 dev/gen-config-docs.py` à partir
des schémas JSON de `package-content/`. Ne jamais l'éditer à la main : modifier le
schéma puis régénérer. `dev/ci.sh` échoue si le fichier committé est périmé.

**Ce qu'on abandonne de l'amont** : leur interdiction du développement IA autonome et
l'obligation de discuter chaque changement sur leur Discord. `CONTRIBUTING.md` reflète
déjà cette position.

## FotonExtractor — obtenir des données vanilla manquantes

Checkout : `/root/FotonExtractor` (mod Fabric Kotlin, cible MC 26.2, build validé).
Le checkout et son dépôt portent peut-être encore l'ancien nom : les renommer une
bonne fois, ce document et `dev/doctor.sh` attendent `FotonExtractor`.

```bash
cd ~/FotonExtractor
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64
./gradlew runServer     # produit run/foton_extractor_output/
./gradlew runDatagen
```

Copier **uniquement** les fichiers nécessaires vers le chemin correspondant du repo :
`foton-registry/build_assets/`, `foton-core/build/`, `foton-core/test_assets/`,
`foton-worldgen/build_assets/`, `foton-utils/build_assets/`.

Si une donnée extraite est fausse ou manquante, corriger l'extracteur et le relancer —
ne jamais écrire la valeur à la main.

## Architecture

```
foton (binaire) → foton-login (connexion) → foton-core (logique de jeu)
  → foton-worldgen → foton-protocol (paquets) → foton-macros
  → foton-registry (données générées) → foton-utils / foton-math → foton-crypto
```

| Zone | Chemin |
|------|--------|
| Logique de jeu | `foton-core/src/` |
| Behaviors blocs | `foton-core/src/behavior/blocks/` |
| Behaviors items | `foton-core/src/behavior/items/` |
| Entités | `foton-core/src/entity/entities/` |
| Block entities | `foton-core/src/block_entity/` |
| Monde / chunks | `foton-core/src/world/`, `foton-core/src/chunk/` |
| Paquets | `foton-protocol/src/packets/game/` |
| Handlers joueur | `foton-core/src/player/networking.rs` |
| Build scripts | `foton-core/build/`, `foton-registry/build/`, `foton-worldgen/build/` |
| Source vanilla | `minecraft-src/minecraft/src/net/minecraft/` |

## Mécanisme d'implémentation — le point central

`foton-core/build/classes.json` associe chaque entrée vanilla à sa classe Java :

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

## État de la couverture

Mesuré par `python3 dev/coverage.py`, qui croise `foton-core/build/classes.json`
avec les structs portant `#[block_behavior]`, `#[item_behavior]` ou
`#[entity_behavior]`. Relancer la commande plutôt que se fier au tableau.

Le registre `dev/parity-gaps.txt`, généré au build et gardé par un test, doit
donner exactement le même résultat. En cas de désaccord, **c'est le registre qui
a raison** : il sort du même codegen que l'enregistrement. Un désaccord réel a
existé — le script ne reconnaissait que la forme nue `#[item_behavior]` et
ratait les deux items livre écrits `#[foton_macros::item_behavior]`.

| Catégorie | Couverture | Au départ du fork |
|-----------|-----------|-------------------|
| Blocs     | 255 / 265 (96 %) | 185 / 265 (70 %) |
| Items     | 69 / 70 (99 %)   | 30 / 70 (43 %)   |
| Entités   | 141 / 142 (99 %) | 4 / 142 (3 %)    |

`dev/coverage.py --list entities` affiche le détail couvert / manquant.

## Systèmes livrés depuis le fork

- **Conteneurs** : coffre (double compris), fourneau / fumoir / haut-fourneau
  avec table de burn time et recettes blasting + smoking, hopper avec accès par
  face (port de `WorldlyContainer`).
- **Combat et mobs hostiles** : zombie, squelette, creeper, araignée, husk,
  araignée venimeuse, vagabond, avec explosions (`ServerExplosion`), flèches, arc,
  combustion au soleil et effets de statut à l'impact.
- **Spawn naturel** : `World::tick_natural_spawn`, listes pondérées par biome.
- **Difficulté locale** : port de `DifficultyInstance`.
- **Animaux** : poulet (ponte comprise) en plus des vache / cochon / mouton.

## Ce qui existe déjà — ne pas réécrire

Le piège serait de reconstruire ces systèmes faute de les avoir cherchés :

- Loot tables : moteur générique (`foton-registry/src/loot_table/`), branché sur
  les blocs, la mort des entités, la tonte et les cadeaux.
- Dégâts, mort, respawn, expérience, sauvegarde Anvil `.mca`.
- Redstone : pistons avec structures collées, comparateurs, rails, observateurs.
- Projectiles : moteur complet (`entity/projectile/mod.rs`).
- Élevage : goals + `AgeableMob` + `Animal` génériques.
- IA : `MeleeAttackGoal`, `HurtByTargetGoal`, `NearestAttackableTargetGoal`,
  `FleeSunGoal`, `RestrictSunGoal`, `LeapAtTargetGoal`, tous exportés et utilisés.
- Menus : coffre, double coffre, fourneau, hopper, enclume, craft, inventaire.
- Effets de statut : `MobEffectInstance`, application et synchronisation.

## Chemin critique restant

Cette section était périmée : les cinq chantiers qu'elle listait (mobs, potions,
conteneurs restants, table d'enchantement, villageois) sont livrés. Vérifié le
2026-08-29 — `BrewingStandBlock`, `EnchantingTableBlock`, `DispenserBlock`,
`DropperBlock`, `EnderChestBlock`, `ShulkerBoxBlock`, `TrappedChestBlock`,
`PotionContents`, `MerchantMenu` et le système de `Brain` existent tous.

Ce que `dev/coverage.py --list` donne encore comme non couvert :

- **Blocs (10)** : `AirBlock`, `Block`, `HalfTransparentBlock`,
  `TransparentBlock` (classes de base vanilla, pas du contenu), puis
  `CryingObsidianBlock`, `StainedGlassBlock`, `TintedGlassBlock`,
  `StructureVoidBlock` et les deux blocs de gametest `TestBlock`,
  `TestInstanceBlock`.
- **Items (1)** : `Item` (classe de base).
- **Entités (1)** : `Player` (porté hors du mécanisme `#[entity_behavior]`).

La couverture par annotation ne dit rien de la *justesse* des implémentations :
`PARITY.md` est l'inventaire qui fait foi sur ce point, et il rappelle qu'un
système qui existe n'est pas un système qui marche.

## Workflow git

- **Une branche par sujet**, jamais de commit direct sur `master`
  (le hook `no-commit-to-branch` de prek le bloque de toute façon).
- Nommage : `feat/<sujet>`, `fix/<sujet>`, `refactor/<sujet>`.
- Messages en **conventional commits** : `feat: implement nylium blocks`,
  `fix(setblock): correct inverted keep mode`.
- **Commits atomiques et fréquents** : un commit = un changement cohérent qui compile.
- Vérifier avant chaque commit : `cargo check` au minimum, la suite CI avant de merger.
- Merge dans `master` une fois la CI verte.

## Licence — AGPL-3.0

Foton dérive d'une base AGPL-3.0 antérieure. Conséquences :

- Foton reste sous **AGPL-3.0**, relicensing propriétaire impossible. Le renommage
  ne change rien : c'est une œuvre dérivée.
- Conserver les mentions de copyright et signaler nos modifications.
- **Clause réseau** : dès qu'un serveur tournant sous ce code est accessible à des tiers,
  ceux-ci ont droit au code source de Foton. Développer en privé n'entraîne
  aucune obligation ; déployer publiquement en entraîne une.
