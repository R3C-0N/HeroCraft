# Objets

Chaque fichier `*.json` de ce dossier décrit un objet de l'inventaire qui n'est pas un bloc : outil, arme, armure, nourriture, matériau, butin, totem… Son nom (sans `.json`) en est l'identifiant. Les icônes sont rangées dans `icones/`. Les blocs, eux, sont décrits dans `assets/blocks/`.

Pour ajouter un objet, il suffit d'ajouter sa fiche et son icône. Un fichier invalide est signalé dans la console au lancement, puis ignoré. Ils se rangent et se déplacent comme les blocs, mais ne se posent pas. Ils tombent du butin des blocs cassés et des bêtes tuées (champ `loot` de leurs fiches) et se fabriquent selon les recettes de leur fiche (`recipes`, voir [assets/stations/README.md](../stations/README.md)) ; le combat et la nourriture restent à faire.

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
| `icon` | Icône PNG, chemin depuis ce dossier. Lue au texel près (sans lissage). |
| `stats` | Caractéristiques affichées dans la bulle, nom et valeur : `Dégâts`, `Durabilité`, `Protection`, `Satiété`… |
| `recipes` | Recettes qui le fabriquent : établi et niveau, ingrédients, nombre fabriqué. Voir [assets/stations/README.md](../stations/README.md). |
