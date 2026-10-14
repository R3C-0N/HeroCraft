// Copyright 2026 R3C-0N
// SPDX-License-Identifier: Apache-2.0
package org.terasology.herocraft.ui.legende;

import org.terasology.engine.entitySystem.prefab.Prefab;
import org.terasology.engine.logic.common.DisplayNameComponent;
import org.terasology.engine.logic.players.LocalPlayer;
import org.terasology.engine.registry.In;
import org.terasology.engine.rendering.nui.CoreScreenLayer;
import org.terasology.engine.utilities.Assets;
import org.terasology.heroclasses.legende.Apparence;
import org.terasology.heroclasses.legende.Atlas;
import org.terasology.heroclasses.legende.BaptemeRequest;
import org.terasology.heroclasses.legende.Espece;
import org.terasology.heroclasses.legende.Forge;
import org.terasology.heroclasses.legende.LegendeComponent;
import org.terasology.heroclasses.legende.Traits;
import org.terasology.heroclasses.legende.Variante;
import org.terasology.heroclasses.skills.ClassComponent;
import org.terasology.heroclasses.skills.SkillCatalog;
import org.terasology.nui.UIWidget;
import org.terasology.nui.databinding.Binding;
import org.terasology.nui.databinding.ReadOnlyBinding;
import org.terasology.nui.layouts.ColumnLayout;
import org.terasology.nui.layouts.FlowLayout;
import org.terasology.nui.layouts.RowLayout;
import org.terasology.nui.layouts.RowLayoutHint;
import org.terasology.nui.layouts.ScrollableArea;
import org.terasology.nui.widgets.TextChangeEventListener;
import org.terasology.nui.widgets.UIBox;
import org.terasology.nui.widgets.UIButton;
import org.terasology.nui.widgets.UILabel;
import org.terasology.nui.widgets.UISlider;
import org.terasology.nui.widgets.UIText;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Random;
import java.util.function.Consumer;
import java.util.function.Function;
import java.util.function.Supplier;

/**
 * The legend creator, the mock-up's three steps in the game: the body at the forge, the way at the rack, the
 * name at the baptism. Nothing is decided here: the last step sends every choice to the server, which seals the
 * hero or says why not (see {@code LegendeAuthoritySystem}).
 * <p>
 * The panel's content is rebuilt on every choice, and only then: a slider being dragged updates the figure and its
 * own value but leaves the panel standing, or the drag would lose its widget under the cursor.
 */
public class LegendeScreen extends CoreScreenLayer {

    private static final String[] TITRES = {"La forge des âmes", "Le râtelier", "Le nom"};
    private static final String[] INTROS = {
        "Rien n’est encore écrit. Une silhouette attend sur l’enclume : donnez-lui une chair, une taille, un regard.",
        "Le maître d’armes ouvre le râtelier. Votre voie décide de ce que vos mains tiendront au premier matin.",
        "Tout est en place, sauf le nom. C’est lui que les tavernes répéteront, ou oublieront."
    };
    private static final String[] PANNEAUX = {"Modelage", "Les voies", "Baptême"};
    private static final String[] PAS = {"Le corps", "La voie", "Le nom"};
    private static final String[][] ONGLETS = {
        {"espece", "Espèce"}, {"corps", "Corps"}, {"visage", "Visage"}, {"pilosite", "Pilosité"}, {"tenue", "Tenue"}
    };
    /** The mock-up's families, in its order, with the line under each name. */
    private static final String[][] FAMILLES = {
        {"magic", "Magie", "On y plie le monde en le nommant."},
        {"ranged", "Combat à distance", "On y frappe avant d’être vu."},
        {"melee", "Corps à corps", "On y prend les coups de face."}
    };

    @In
    private LocalPlayer localPlayer;
    @In
    private SkillCatalog catalog;

    private final Random hasard = new Random();

    private ColumnLayout contenu;
    private ScrollableArea defilement;
    private RowLayout pas;
    private ApercuHeros apercu;
    private UIButton suivant;
    private UIButton retour;
    private Case rotation;

    private int etape;
    private String onglet = "espece";
    private Apparence cfg = Atlas.defaut("humain");
    private Prefab classe;
    private final Map<String, String> armes = new HashMap<>();
    private String nom = "";
    private String refus = "";
    private boolean envoye;

    @Override
    public void initialise() {
        contenu = find("contenu", ColumnLayout.class);
        contenu.setFillVerticalSpace(false);
        defilement = find("defilement", ScrollableArea.class);
        pas = find("pas", RowLayout.class);
        apercu = find("apercu", ApercuHeros.class);
        suivant = find("suivant", UIButton.class);
        retour = find("retour", UIButton.class);
        rotation = find("rotation", Case.class);

        suivant.subscribe(widget -> avancer());
        retour.subscribe(widget -> aller(etape - 1));
        rotation.regler("Rotation", "", apercu::isTourne, () -> apercu.setTourne(!apercu.isTourne()));

        lier("titre", () -> TITRES[etape]);
        lier("intro", () -> INTROS[etape]);
        lier("panneauTitre", () -> PANNEAUX[etape]);
        lier("pied", this::pied);
        lier("infoTaille", () -> metres(cfg.taille) + " · " + corpulence().toLowerCase(Locale.FRENCH));
        lier("infoTitre", () -> classe == null ? espece().nom() : espece().nom() + " · " + voie().name);
        lier("infoVariante", () -> variante().nom());
        lier("infoApparence", () -> classe != null ? voie().role
                : variante().apparence() != null ? variante().apparence() : espece().apparence());
        lier("infoMains", this::mains);
        suivant.bindText(new ReadOnlyBinding<String>() {
            @Override
            public String get() {
                if (scelle()) {
                    return "Entrer dans le monde";
                }
                return etape == 0 ? "Choisir sa voie" : etape == 1 ? "Nommer le héros" : "Sceller la légende";
            }
        });

        rafraichir();
    }

    @Override
    public void onOpened() {
        super.onOpened();
        envoye = false;
        refus = "";
        rafraichir();
    }

    @Override
    public boolean isModal() {
        return true;
    }

    /** The server's word on a baptism it turned down. */
    public void refuser(String raison) {
        envoye = false;
        refus = "Le sceau ne prend pas : " + raison + ".";
        rafraichir();
    }

    /** The server sealed the hero: the last step says so and the button now leads into the world. */
    public void scellee() {
        envoye = false;
        rafraichir();
    }

    // ---------------------------------------------------------------------------------------------- navigation

    private void aller(int vers) {
        if (scelle()) {
            return;
        }
        int max = classe == null ? 1 : 2;
        etape = Math.max(0, Math.min(max, vers));
        refus = "";
        defilement.moveToTop();
        rafraichir();
    }

    private void avancer() {
        if (scelle()) {
            getManager().closeScreen(this);
        } else if (etape < 2) {
            aller(etape + 1);
        } else if (!nom.trim().isEmpty() && classe != null && !envoye) {
            Apparence a = cfg.copie();
            a.nom = nom.trim();
            envoye = true;
            refus = "";
            localPlayer.getCharacterEntity().send(new BaptemeRequest(a, classe.getUrn().toString(), arme()));
            rafraichir();
        }
    }

    private boolean scelle() {
        LegendeComponent legende = localPlayer.getCharacterEntity().getComponent(LegendeComponent.class);
        return legende != null && legende.scelle;
    }

    private String pied() {
        if (scelle()) {
            return "Fiche scellée.";
        }
        if (envoye) {
            return "Le sceau se pose…";
        }
        if (etape == 0) {
            return "Étape 1 sur 3 — le corps.";
        }
        if (etape == 1) {
            return classe == null ? "Choisissez une voie pour continuer." : "Voie choisie : " + voie().name + ".";
        }
        return "Un nom, et la partie commence.";
    }

    // ---------------------------------------------------------------------------------------------- rebuilding

    private void rafraichir() {
        if (contenu == null) {
            return;
        }
        apercu.montrer(cfg);
        apercu.armer(arme());
        boolean fige = scelle() || envoye;
        retour.setEnabled(etape > 0 && !fige);
        suivant.setEnabled(scelle() || (!envoye && (etape == 0 || classe != null)
                && (etape < 2 || !nom.trim().isEmpty())));

        pas.removeAllWidgets();
        for (int i = 0; i < PAS.length; i++) {
            int vers = i;
            UIButton bouton = new UIButton("", (i + 1) + "  " + PAS[i].toUpperCase(Locale.FRENCH));
            bouton.setFamily(i == etape ? "pasActif" : "pas");
            bouton.setEnabled(!fige && (i < 2 || classe != null));
            bouton.subscribe(widget -> aller(vers));
            pas.addWidget(bouton, new RowLayoutHint());
        }

        contenu.removeAllWidgets();
        if (etape == 0) {
            forge();
        } else if (etape == 1) {
            ratelier();
        } else {
            bapteme();
        }
    }

    /** Step 1: the tabs, then what the open tab shapes, then the dice. */
    private void forge() {
        RowLayout onglets = new RowLayout();
        onglets.setHorizontalSpacing(4);
        for (String[] o : ONGLETS) {
            UIButton bouton = new UIButton("", o[1].toUpperCase(Locale.FRENCH));
            bouton.setFamily(o[0].equals(onglet) ? "ongletActif" : "onglet");
            bouton.subscribe(widget -> {
                onglet = o[0];
                rafraichir();
            });
            onglets.addWidget(bouton, new RowLayoutHint());
        }
        contenu.addWidget(onglets);

        Traits traits = Atlas.traitsDe(cfg);
        switch (onglet) {
            case "espece" -> {
                ColumnLayout grille = new ColumnLayout();
                grille.setColumns(2);
                grille.setHorizontalSpacing(8);
                grille.setVerticalSpacing(6);
                grille.setFillVerticalSpace(false);
                for (String id : Atlas.ORDRE_ESPECES) {
                    Espece e = Atlas.espece(id);
                    grille.addWidget(new Case(e.nom(), metres(e.taille()), () -> id.equals(cfg.espece), () -> {
                        cfg = Atlas.defaut(id);
                        rafraichir();
                    }));
                }
                contenu.addWidget(grille);
                List<Option> variantes = new ArrayList<>();
                List<Variante> liste = espece().variantes();
                for (int i = 0; i < liste.size(); i++) {
                    variantes.add(new Option(Integer.toString(i), liste.get(i).nom()));
                }
                options("Variante", variantes, Integer.toString(cfg.variante), v -> {
                    cfg.variante = Integer.parseInt(v);
                    Forge.retoucher(cfg, true);
                }, "Chaque variante est une sous-race : silhouette, carnation et appendices lui sont propres.");
            }
            case "corps" -> {
                Espece e = espece();
                curseur("Taille", (float) e.min(), (float) e.max(), 0.01f, () -> (float) cfg.taille,
                        v -> cfg.taille = v, () -> metres(cfg.taille),
                        "Entre " + metres(e.min()) + " et " + metres(e.max()) + " pour cette espèce.");
                curseur("Corpulence", 0, 1, 0.02f, () -> (float) cfg.corpulence, v -> cfg.corpulence = v,
                        this::corpulence, "");
                couleurs("Couleur de peau", Atlas.peauxDe(cfg), () -> cfg.peau, c -> cfg.peau = c);
                options("Forme de tête", options(Atlas.FORMES_TETE, f -> f.id(), f -> f.nom()), cfg.formeTete,
                        v -> cfg.formeTete = v, "");
            }
            case "visage" -> {
                options("Yeux", options(Atlas.YEUX, x -> x.id(), x -> x.nom()), cfg.yeux, v -> cfg.yeux = v, "");
                couleurs("Couleur des yeux", Atlas.PAL_YEUX, () -> cfg.couleurYeux, c -> cfg.couleurYeux = c);
                if ("lumineux".equals(traits.regard())) {
                    note("Regard lumineux sans pupille : la teinte vient de la variante.");
                }
                options("Bouche", options(Atlas.BOUCHES, x -> x.id(), x -> x.nom()), cfg.bouche,
                        v -> cfg.bouche = v,
                        traits.canines() != null ? "Canines inférieures saillantes ajoutées par l’espèce." : "");
                options("Marques", options(Atlas.MARQUES, x -> x.id(), x -> x.nom()), cfg.marques,
                        v -> cfg.marques = v, "");
            }
            case "pilosite" -> {
                options("Coiffure", options(Atlas.COIFFURES, x -> x.id(), x -> x.nom()), cfg.coiffure,
                        v -> cfg.coiffure = v, traits.coiffureForcee() != null ? "Imposée par la variante." : "");
                couleurs("Couleur de cheveux", Atlas.PAL_CHEVEUX, () -> cfg.couleurCheveux,
                        c -> cfg.couleurCheveux = c);
                options("Barbe", options(Atlas.BARBES, x -> x.id(), x -> x.nom()), cfg.barbe, v -> cfg.barbe = v,
                        traits.barbeForcee() != null ? "Pilosité imposante : imposée par la variante." : "");
            }
            default -> {
                options("Vêtements de base", options(Atlas.TENUES, x -> x.id(), x -> x.nom()), cfg.tenue,
                        v -> cfg.tenue = v, "");
                couleurs("Couleur de tenue", Atlas.PAL_TENUE, () -> cfg.couleurTenue, c -> cfg.couleurTenue = c);
            }
        }

        RowLayout pied = new RowLayout();
        pied.setHorizontalSpacing(12);
        UILabel avertissement = new UILabel("Aucun trait n’est définitif avant le baptême.");
        avertissement.setFamily("small-muted");
        pied.addWidget(avertissement, new RowLayoutHint(0.7f));
        UIButton des = new UIButton("", "Tirer au sort");
        des.setFamily("fantome");
        des.subscribe(widget -> {
            cfg = Atlas.aleatoire(hasard);
            rafraichir();
        });
        pied.addWidget(des, new RowLayoutHint(0.3f));
        contenu.addWidget(pied);
    }

    /** Step 2: the fifteen ways by family, then the one taken — its line, its arms, what it puts in the bag. */
    private void ratelier() {
        List<Prefab> voies = catalog.classes();
        for (String[] famille : FAMILLES) {
            RowLayout tete = new RowLayout();
            tete.setHorizontalSpacing(12);
            UILabel titre = new UILabel(famille[1].toUpperCase(Locale.FRENCH));
            titre.setFamily("rubrique-or");
            tete.addWidget(titre, new RowLayoutHint(0.34f));
            UILabel ligne = new UILabel(famille[2]);
            ligne.setFamily("small-muted");
            tete.addWidget(ligne, new RowLayoutHint(0.66f));
            contenu.addWidget(tete);

            ColumnLayout grille = new ColumnLayout();
            grille.setColumns(2);
            grille.setHorizontalSpacing(8);
            grille.setVerticalSpacing(6);
            grille.setFillVerticalSpace(false);
            for (Prefab prefab : voies) {
                ClassComponent voie = prefab.getComponent(ClassComponent.class);
                if (!famille[0].equals(voie.category)) {
                    continue;
                }
                grille.addWidget(new Case(voie.name, "", () -> prefab.equals(classe), () -> {
                    classe = prefab;
                    refus = "";
                    rafraichir();
                }));
            }
            contenu.addWidget(grille);
        }
        if (classe == null) {
            return;
        }

        ClassComponent voie = voie();
        ColumnLayout fiche = boite();
        UILabel nomVoie = new UILabel(voie.name);
        nomVoie.setFamily("voie");
        fiche.addWidget(nomVoie);
        fiche.addWidget(texte("« " + voie.role + " »", "citation"));
        fiche.addWidget(texte(voie.armament, "small-secondary"));

        if (!voie.arsenal.isEmpty()) {
            fiche.addWidget(rubrique("ARSENAL"));
            FlowLayout choix = new FlowLayout();
            choix.setHorizontalSpacing(8);
            choix.setVerticalSpacing(8);
            for (String item : voie.arsenal) {
                choix.addWidget(new Case(nomObjet(item), "", () -> item.equals(arme()), () -> {
                    armes.put(classe.getUrn().toString(), item);
                    rafraichir();
                }), null);
            }
            fiche.addWidget(choix);
        }

        fiche.addWidget(rubrique("AU PREMIER MATIN"));
        for (String ligne : paquetage()) {
            fiche.addWidget(texte("·  " + ligne, "small-secondary"));
        }
    }

    /** A sunken box in the panel, the mock-up's well for whatever is read rather than picked. */
    private ColumnLayout boite() {
        ColumnLayout colonne = new ColumnLayout();
        colonne.setVerticalSpacing(8);
        colonne.setFillVerticalSpace(false);
        UIBox boite = new UIBox();
        boite.setFamily("sombre");
        boite.setContent(colonne);
        contenu.addWidget(boite);
        return colonne;
    }

    /** Step 3: the name, and the sheet it will be written on. */
    private void bapteme() {
        RowLayout champ = new RowLayout();
        champ.setHorizontalSpacing(12);
        UIText saisie = new UIText();
        saisie.setFamily("nom");
        saisie.setText(nom);
        saisie.setEnabled(!scelle() && !envoye);
        saisie.subscribe((TextChangeEventListener) (ancien, nouveau) -> {
            nom = nouveau.length() > 32 ? nouveau.substring(0, 32) : nouveau;
            refus = "";
            suivant.setEnabled(!envoye && !nom.trim().isEmpty());
        });
        champ.addWidget(saisie, new RowLayoutHint(0.68f));
        UIButton tirer = new UIButton("", "Tirer un nom");
        tirer.setFamily("fantome");
        tirer.setEnabled(!scelle() && !envoye);
        tirer.subscribe(widget -> {
            nom = Forge.nomAleatoire(hasard);
            refus = "";
            rafraichir();
        });
        champ.addWidget(tirer, new RowLayoutHint(0.32f));
        contenu.addWidget(rubrique("NOM DU HÉROS"));
        contenu.addWidget(champ);
        contenu.addWidget(texte("Un nom se porte longtemps. Les crieurs le répéteront mal, les bardes l’allongeront, "
                + "les registres de garnison l’abrégeront.", "small-muted"));

        ColumnLayout feuille = boite();
        feuille.addWidget(rubrique("FEUILLE DE DÉPART"));
        Variante variante = variante();
        ligne(feuille, "Espèce", espece().nom() + " · " + variante.nom());
        ligne(feuille, "Taille", metres(cfg.taille) + " · corpulence " + corpulence().toLowerCase(Locale.FRENCH));
        ligne(feuille, "Voie", voie().name);
        ligne(feuille, "Armement", voie().armament);
        ligne(feuille, "Palier", "1 · première sortie");

        if (scelle()) {
            LegendeComponent legende = localPlayer.getCharacterEntity().getComponent(LegendeComponent.class);
            UILabel ainsi = new UILabel("Ainsi commence la légende de " + legende.nom + ".");
            ainsi.setFamily("voie");
            contenu.addWidget(ainsi);
            contenu.addWidget(texte("La garnison de Valbrume vous inscrit au palier 1. Le monde ne connaît pas encore "
                    + "votre nom : il l’apprendra, ou il l’oubliera.", "small-secondary"));
        }
        if (!refus.isEmpty()) {
            contenu.addWidget(texte(refus, "refus"));
        }
    }

    // ---------------------------------------------------------------------------------------------- groups

    private record Option(String id, String nom) {
    }

    private static <T> List<Option> options(List<T> liste, Function<T, String> id, Function<T, String> nom) {
        List<Option> options = new ArrayList<>();
        for (T t : liste) {
            options.add(new Option(id.apply(t), nom.apply(t)));
        }
        return options;
    }

    /** A list stepped through with ‹ and ›, the mock-up's selector: the pick, and where it stands in the list. */
    private void options(String label, List<Option> liste, String courant, Consumer<String> choisir, String note) {
        int i = 0;
        for (int k = 0; k < liste.size(); k++) {
            if (liste.get(k).id().equals(courant)) {
                i = k;
            }
        }
        int ici = i;
        entete(label, (i + 1) + " / " + liste.size());
        RowLayout rang = new RowLayout();
        rang.setHorizontalSpacing(8);
        rang.addWidget(fleche("<", () -> choisir.accept(liste.get((ici - 1 + liste.size()) % liste.size()).id())),
                new RowLayoutHint(0.08f));
        UILabel choix = new UILabel(liste.isEmpty() ? "" : liste.get(i).nom());
        choix.setFamily("choix");
        rang.addWidget(choix, new RowLayoutHint(0.84f));
        rang.addWidget(fleche(">", () -> choisir.accept(liste.get((ici + 1) % liste.size()).id())),
                new RowLayoutHint(0.08f));
        contenu.addWidget(rang);
        note(note);
    }

    private UIButton fleche(String signe, Runnable action) {
        UIButton bouton = new UIButton("", signe);
        bouton.setFamily("fleche");
        bouton.subscribe(widget -> {
            action.run();
            Forge.retoucher(cfg, false);
            rafraichir();
        });
        return bouton;
    }

    private void couleurs(String label, List<String> palette, Supplier<String> courante, Consumer<String> choisir) {
        entete(label, courante.get().toUpperCase(Locale.ROOT));
        FlowLayout nuancier = new FlowLayout();
        nuancier.setHorizontalSpacing(6);
        nuancier.setVerticalSpacing(6);
        for (String hex : palette) {
            nuancier.addWidget(new Pastille(hex, () -> hex.equalsIgnoreCase(courante.get()), () -> {
                choisir.accept(hex);
                Forge.retoucher(cfg, false);
                rafraichir();
            }), null);
        }
        contenu.addWidget(nuancier);
    }

    private void curseur(String label, float min, float max, float increment, Supplier<Float> lire, Consumer<Float> ecrire,
                         Supplier<String> valeur, String note) {
        RowLayout tete = new RowLayout();
        tete.addWidget(rubrique(label), new RowLayoutHint());
        UILabel chiffre = new UILabel();
        chiffre.setFamily("valeur");
        chiffre.bindText(new ReadOnlyBinding<String>() {
            @Override
            public String get() {
                return valeur.get();
            }
        });
        tete.addWidget(chiffre, new RowLayoutHint());
        contenu.addWidget(tete);

        UISlider curseur = new UISlider();
        curseur.setMinimum(min);
        curseur.setRange(max - min);
        curseur.setIncrement(increment);
        curseur.setPrecision(2);
        curseur.setLabelFunction(v -> "");
        curseur.bindValue(new Binding<Float>() {
            @Override
            public Float get() {
                return lire.get();
            }

            @Override
            public void set(Float value) {
                ecrire.accept(value);
                Forge.retoucher(cfg, false);
                apercu.montrer(cfg);
            }
        });
        contenu.addWidget(curseur);
        note(note);
    }

    private void entete(String label, String valeur) {
        RowLayout tete = new RowLayout();
        tete.addWidget(rubrique(label), new RowLayoutHint());
        UILabel chiffre = new UILabel(valeur);
        chiffre.setFamily("valeur");
        tete.addWidget(chiffre, new RowLayoutHint());
        contenu.addWidget(tete);
    }

    private void note(String note) {
        if (note != null && !note.isEmpty()) {
            contenu.addWidget(texte(note, "small-muted"));
        }
    }

    private static void ligne(ColumnLayout feuille, String label, String valeur) {
        RowLayout rang = new RowLayout();
        rang.setHorizontalSpacing(12);
        UILabel gauche = new UILabel(label.toUpperCase(Locale.FRENCH));
        gauche.setFamily("cle");
        rang.addWidget(gauche, new RowLayoutHint(0.3f));
        UILabel droite = new UILabel(valeur);
        droite.setFamily("droite");
        rang.addWidget(droite, new RowLayoutHint(0.7f));
        feuille.addWidget(rang);
    }

    private static UILabel rubrique(String texte) {
        UILabel label = new UILabel(texte.toUpperCase(Locale.FRENCH));
        label.setFamily("rubrique");
        return label;
    }

    private static UIWidget texte(String texte, String famille) {
        UILabel label = new UILabel(texte);
        label.setFamily(famille);
        return label;
    }

    private void lier(String id, Supplier<String> texte) {
        find(id, UILabel.class).bindText(new ReadOnlyBinding<String>() {
            @Override
            public String get() {
                return texte.get();
            }
        });
    }

    // ---------------------------------------------------------------------------------------------- reading

    private Espece espece() {
        return Atlas.espece(cfg.espece);
    }

    private Variante variante() {
        return Atlas.varianteDe(cfg);
    }

    private ClassComponent voie() {
        return classe.getComponent(ClassComponent.class);
    }

    private String corpulence() {
        return cfg.corpulence < 0.3 ? "Frêle" : cfg.corpulence > 0.7 ? "Massive" : "Moyenne";
    }

    /** The arsenal pick for the way taken, its first weapon until another is picked; empty without an arsenal. */
    private String arme() {
        if (classe == null || voie().arsenal.isEmpty()) {
            return "";
        }
        return armes.getOrDefault(classe.getUrn().toString(), voie().arsenal.get(0));
    }

    /** What the baptism will put in the bag: the kit, and the arsenal pick. */
    private List<String> paquetage() {
        List<String> lignes = new ArrayList<>();
        if (!arme().isEmpty()) {
            lignes.add(nomObjet(arme()));
        }
        for (ClassComponent.KitItem kit : voie().kit) {
            lignes.add(kit.count > 1 ? nomObjet(kit.item) + " × " + kit.count : nomObjet(kit.item));
        }
        return lignes;
    }

    private String mains() {
        if (classe == null) {
            return "Vides. Aucune voie n’a encore été choisie.";
        }
        return String.join(" · ", paquetage());
    }

    private static String nomObjet(String prefab) {
        return Assets.getPrefab(prefab)
                .map(p -> p.getComponent(DisplayNameComponent.class))
                .map(d -> d.name)
                .orElse(prefab);
    }

    private static String metres(double metres) {
        return String.format(Locale.FRANCE, "%.2f m", metres);
    }
}
