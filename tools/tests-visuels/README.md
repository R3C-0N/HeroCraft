# Tests visuels

Ce dossier lance le vrai client dans Docker, sans carte graphique. Le rendu passe par Vulkan
logiciel (lavapipe), l'écran est virtuel (Xvfb) et xdotool joue le clavier. Le script prend des
captures de ce qui a été ajouté au jeu. Sur un serveur à 12 cœurs, on tourne à environ 2 à 3
images par seconde, ce qui suffit pour les captures.

## Utilisation

Copier ce dossier sur la machine de test, par exemple dans `~/herocraft-test`, avec le dépôt
cloné dans `~/app/HeroCraft`. Construire ensuite l'image une fois :

```
docker build -t herocraft-test .
```

Puis lancer les tests :

```
./tout-tester.sh             # git pull, compilation, tous les scénarios, galerie
./tout-tester.sh 01 04       # seulement les scénarios 01 et 04
PAS_DE_MAJ=1 ./tout-tester.sh
CULTURES="nain gnome" ./tout-tester.sh 02
```

Les captures arrivent dans `shots/<scénario>/`, et la galerie se trouve dans `shots/index.html`.
Chaque scénario repart d'un monde vierge (graine 4242). Le héros, déjà créé, vient de `modele/`.

## Scénarios

| Fichier | Contenu | Durée |
|---|---|---|
| `00-demarrage` | La Genèse, l'arrivée dans le monde, la carte et la roue des compétences | 4 min |
| `01-villes` | Une ville de chaque grande culture : au sol, en troisième personne et vue du ciel | 8 min |
| `02-villages` | Un village de chacune des 10 cultures | 21 min |
| `03-temple` | Les ruines d'un temple, les Pierres du Néant dans le sac et au sol | 2 min |
| `04-souvenir` | `/vision` : L'éclat, le tutoriel, les combattants de l'assaut, le retour | 6 min |
| `05-cinematiques` | Les cinématiques ajoutées, jouées avec `/cinematic` | 6 min |

## Écrire un scénario

Un scénario est un fichier bash qui utilise les fonctions de `lib.sh` :

- `hc_start ARGS` lance le jeu sans la Genèse (`--sans-genese`), attend le chargement et règle l'heure sur midi.
  Exemples d'arguments : `--visiter ville|village|ruines|temple [rang] [--culture nain]`.
- `hc_cmd "/commande"` envoie une commande du chat. Elle passe par le fichier `HEROCRAFT_COMMANDES`, que le jeu lit à chaque image.
- `hc_cam DISTANCE [YAW|'~'] [PITCH|'~']` règle la caméra (commande `/camera`). Le `~` doit être entre guillemets.
- `hc_shot NOM` prend une capture, et `hc_series NOM N PAS` en prend N espacées de PAS secondes.
- `hc_tour NOM` fait le tour d'un lieu : au sol, en troisième personne, puis trois vues du ciel.
- `hc_key`, `hc_hold TOUCHE SECONDES` et `hc_click X Y` simulent le clavier et la souris. Le clavier est en QWERTY : W A S D.

`session.sh` ouvre une partie pilotable à la main pour mettre un scénario au point :

```
./session.sh start essai --seed 4242 --visiter temple
./session.sh do essai 'hc_cmd "/vision"; sleep 5; hc_shot a1'
./session.sh stop essai
```
