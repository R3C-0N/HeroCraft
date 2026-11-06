# Cultures

Chaque fichier `*.json` de ce dossier décrit une plante cultivée. Son nom (sans `.json`) en est l'identifiant.

Une graine (l'objet `seed`) se sème au clic droit sur une terre labourée (`terreLabouree`, ou `terreLaboureeHumide` près de l'eau, faites à la houe) : le premier bloc de `stages` se pose dessus, puis la plante passe d'un stade au suivant, jusqu'au dernier, la plante mûre. Ce que donne chaque stade cassé est décrit dans la fiche de son bloc (`loot`, voir [assets/blocks/README.md](../blocks/README.md)).

Pour ajouter une culture, il suffit d'ajouter sa fiche (et les blocs de ses stades, et l'objet de sa graine). Un fichier invalide est signalé dans la console au lancement, puis ignoré.

## Exemple

```json
{
  "name": "Riz",
  "seed": "grainesRiz",
  "stages": ["rizStade1", "rizStade2", "rizStade3", "rizStade4"],
  "stage_minutes": 3.0,
  "needs_water": true
}
```

## Champs

| Champ | Rôle |
|---|---|
| `name` | Nom affiché. |
| `seed` | Identifiant de l'objet qui la sème. |
| `stages` | Blocs de ses stades, du semis à la plante mûre (au moins deux). Un bloc ne sert qu'à une culture. |
| `stage_minutes` | Durée d'un stade sur une terre labourée humide, en minutes. Sur une terre sèche, deux fois plus. |
| `needs_water` | `true` : elle ne pousse que sur une terre humide (le riz). Faux par défaut. |
