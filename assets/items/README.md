# Objets

Chaque fichier `*.json` de ce dossier décrit un objet de l'inventaire qui n'est pas un bloc : outil, arme, armure, nourriture, matériau, butin, totem… Son nom (sans `.json`) en est l'identifiant. Les icônes sont rangées dans `icones/`. Les blocs, eux, sont décrits dans `assets/blocks/`.

Pour ajouter un objet, il suffit d'ajouter sa fiche et son icône. Un fichier invalide est signalé dans la console au lancement, puis ignoré. Ils se rangent et se déplacent comme les blocs, mais ne se posent pas. Ils se fabriquent selon les recettes de leur fiche (`recipes`, voir [assets/stations/README.md](../stations/README.md)) ; les armures et les bijoux se portent dans les cases d'équipement de l'écran d'inventaire (`slot`) ; le combat reste à faire. Un objet qui a une caractéristique `Satiété` se mange : en main, au clic droit, il remplit d'autant la jauge de faim.

Les 523 objets actuels viennent des modules de la branche `develop` (`CoreAssets`, `HeroEquipment`, `HeroSurvival`, `HeroBestiary`). Le script `tools/importer_blocs_objets.py` les reprend : il convertit les prefabs `engine:iconItem` et découpe les icônes dans leurs atlas. Le relancer après une retouche des modules remet ces fichiers à jour sans changer les numéros.

## Exemple

```json
{
  "numeric_id": 16486,
  "name": "Glaive en silex",
  "description": "Une lame grossière, mais tranchante.",
  "category": "Armes",
  "max_stack": 1,
  "icon": "icones/flintSword.png",
  "stats": {"Dégâts": 4, "Durabilité": 150}
}
```

## Champs

| Champ | Rôle |
|---|---|
| `numeric_id` | Numéro de l'objet dans les inventaires enregistrés, à partir de 16384 (en dessous, ce sont les blocs). **Ne plus le changer** une fois des inventaires enregistrés. |
| `name`, `description` | Nom affiché et description (bulle de l'inventaire). |
| `category` | Rubrique (`Armes`, `Armures`, `Outils`, `Plats`, `Butin`…). |
| `max_stack` | Nombre au plus par case, de 1 à 999 (64 par défaut, 1 pour une arme ou une armure). |
| `slot` | Emplacement d'équipement où l'objet se porte : `head` (tête), `chest` (torse), `legs` (jambes), `feet` (pieds), `cape`, `amulet` ou `ring` (anneau, deux cases). Absent pour ce qui ne se porte pas. Un objet qui se porte ne s'empile pas (`max_stack` à 1). |
| `icon` | Icône PNG, chemin depuis ce dossier. Lue au texel près (sans lissage). |
| `stats` | Caractéristiques affichées dans la bulle, nom et valeur : `Dégâts`, `Durabilité`, `Protection`, `Satiété`… |
| `recipes` | Recettes qui le fabriquent : établi et niveau, ingrédients, nombre fabriqué. Voir [assets/stations/README.md](../stations/README.md). |
