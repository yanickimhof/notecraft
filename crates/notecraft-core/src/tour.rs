//! The page a new workspace opens on: a tour of everything NøteCraft can do, written with the
//! app's own blocks so each line is also an example. English or French.

use crate::blocks::{from_markdown, Block, BlockKind};
use crate::database::{PropKind, PropValue, View, ViewKind};
use crate::workspace::Workspace;
use crate::Result;

const LOGO: &[u8] = include_bytes!("../../../assets/icon-1024.png");

const FR: &str = r####"Cette page est écrite dans NøteCraft. Chaque bloc est un exemple de ce que tu peux faire, et tu peux tout modifier ou la supprimer quand tu veux. Elle est aussi un fichier Markdown ordinaire dans ton dossier.

> [!NOTE]
> 💡 Clique n'importe où et écris. Tape **/** pour ajouter un bloc, **⌘P** pour trouver une page, **⌘,** pour les réglages.

<!--toc-->

## Écrire

Du texte avec du **gras**, de l'*italique*, du <u>souligné</u>, du ~~barré~~, du `code en ligne` et un [lien](https://github.com/yanickimhof/notecraft). Les raccourcis habituels (⌘B, ⌘I, ⌘U, ⌘E) et le Markdown fonctionnent partout. Sélectionne un mot pour changer sa couleur.

- Une liste à puces
  - avec des sous-niveaux
- Une deuxième idée

1. Une liste numérotée
2. Qui se renumérote toute seule

- [x] Une tâche terminée
- [ ] Une tâche à faire

> Une citation, pour mettre une idée en avant.

Un paragraphe en couleur, pour attirer l'oeil. <!--c:red-->

Un paragraphe sur fond coloré. <!--c:bg-yellow-->

---

<details><summary>Un bloc repliable</summary>

Le contenu se cache ou se montre d'un clic. Pratique pour les détails, les FAQ ou les longues listes.

</details>

> [!NOTE]
> ⚠️ Un encadré pour une mise en garde. Il en existe de toutes les couleurs, avec l'icône de ton choix.

```rust
fn main() {
    println!("Du code, avec sa langue indiquée");
}
```

## Mettre en page

<!--columns-->

### À gauche

Glisse un bloc sur le bord d'un autre pour faire des colonnes. Chaque colonne est indépendante.

<!--column-->

### Au milieu

Les colonnes s'empilent toutes seules sur un téléphone, quand la page est publiée.

<!--column-->

### À droite

Utile pour comparer, présenter ou aérer une page.

<!--/columns-->

| Fonction | Dans l'app | Sur la page publiée |
|---|---|---|
| Texte riche | Oui | Oui |
| Colonnes | Oui | Oui |
| Images | Oui, redimensionnables | Oui |
| Mode sombre | Oui | Oui, automatique |

![Le logo de NøteCraft](@LOGO@)<!--w:25-->

Tire la poignée ronde au coin d'une image pour la redimensionner, clic droit pour l'aligner à gauche, au centre ou à droite. Glisse un fichier dans la page pour le joindre.

<!--bookmark:https://github.com/yanickimhof/notecraft NøteCraft sur GitHub-->

## Relier ses idées

- **Pages dans les pages** : voir [[Une page dans la page]], dans l'arbre à gauche. Glisse-les pour les ranger.
- **Liens** : écris `[[` puis le nom d'une page. L'autre page affiche un rétrolien.
- **Graphe** : l'entrée « Graphe » de la barre latérale montre les liens entre tes pages.
- **Icônes, couvertures, favoris** : dans le menu de la page (•••). La corbeille garde tout et permet de restaurer.
- **Onglets** : ⌘T ouvre un onglet, ⌘ clic sur une page l'ouvre dans un nouvel onglet. Ils reviennent au redémarrage.
- **Rappels** : sur une page, menu (•••), Rappel. L'entrée « Rappels » les liste tous.

## Étudier et visualiser

Une équation, résolue pas à pas (tape `/equation`) :

$$
x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
$$

Un schéma dessiné depuis du texte (tape `/mermaid`) :

```mermaid
graph LR
  A[Idée] --> B[Note] --> C[Page structurée] --> D[Publication]
```

```mermaid
sequenceDiagram
  Toi->>NøteCraft: Une idée
  NøteCraft->>Toi: Une page propre
```

```mermaid
pie title Où va le temps
  "Écrire" : 45
  "Organiser" : 30
  "Relire" : 25
```

## Bases de données

Une base de données peut vivre dans une page, avec ses vues Tableau, Tableau de bord, Kanban, Liste, Calendrier, Galerie, Graphique et Frise. Chaque ligne est aussi une page complète. Essaie de changer de vue :

<!--database:Projets-->

Propriétés : texte, nombre, sélection, sélection multiple, statut, date, case à cocher, lien, e-mail, téléphone, relation, personne, fichiers, identifiant, dates de création et de modification, formule et cumul. Filtres, tris, regroupements et modèles de ligne sont disponibles.

## Retrouver

⌘P ouvre la recherche rapide. La recherche plein texte couvre toutes les pages et toutes les bases, et répond pendant que tu tapes.

## Protéger

- **Pages privées** : verrouille une page avec une phrase secrète (chiffrement fort). Une phrase perdue ne se récupère pas.
- **Lecture seule** : pour les pages qu'on ne veut pas modifier par erreur.
- **Sauvegardes automatiques** avec points de restauration, et **historique des versions** de chaque page.
- **Sauvegarde sûre** : une synchronisation ratée n'écrase jamais ton travail en silence, une « copie en conflit » est gardée.

## Importer et exporter

- **Importer** depuis Evernote, Obsidian, Bear, Joplin, Word, HTML, CSV et Markdown (barre latérale, Importer). Un rapport liste ce qui a été ignoré.
- **Exporter** une page en Markdown, PDF, Word ou HTML, ou tout l'espace en Markdown.

## Partager

- **Présentation** (⌥⌘P) : la page s'ouvre dans ton navigateur, telle qu'elle sera publiée. Défile, zoome, plein écran avec F.
- **Publier** (menu •••) : la page part sur un lien privé impossible à deviner, gratuitement, sur ton propre compte Cloudflare. **Aperçu** d'abord, **Mettre à jour** ensuite, **Dépublier** quand tu veux. Réglage une seule fois dans Réglages, Publication.

## Travailler avec l'IA

- **⌘J** : résume, continue, améliore, traduis ou pose une question sur la page. Tu vois le résultat avant que rien ne change.
- **Ollama** (tout reste sur ton Mac) ou ta propre clé **Claude**, **ChatGPT** ou **Grok**. Choisis le moteur et le modèle dans Réglages.
- **MCP** : Claude Desktop et Claude Code peuvent lire et modifier tes notes, avec les droits que tu choisis (lecture seule, lecture et écriture, suppression) et, si tu veux, limités à une page et ses sous-pages.

## Réunions

Tape `/meeting` et lance l'enregistrement : le micro, le son du Mac (Teams, Zoom, Meet) ou les deux. La parole est transcrite sur ton Mac, les intervenants sont distingués et renommables, et l'IA écrit le compte rendu avec un modèle au choix (synthèse, procès-verbal, entretien individuel, vente, cours…). Un vocabulaire personnalisé aide à bien écrire les noms propres.

## Raccourcis à retenir

| Touches | Action |
|---|---|
| ⌘P | Chercher une page |
| ⌘N | Nouvelle page |
| ⌘T, ⌘W | Nouvel onglet, fermer |
| ⌥⌘P | Présenter |
| ⌘J | Assistant IA |
| ⌘/ | Transformer le bloc |
| ⌘D | Dupliquer le bloc |
| ⌘⇧L | Clair ou sombre |
| ⌘, | Réglages |

## Chez toi

Tes notes sont des fichiers ordinaires dans le dossier que tu as choisi : sauvegarde-le comme n'importe quel dossier, ou garde-le dans iCloud Drive, Dropbox ou kDrive pour plusieurs Mac. Pas de compte, pas de suivi, pas d'abonnement. Une ligne de commande `notecraft` permet de chercher, importer, exporter et sauvegarder. Tu peux supprimer cette page et ses exemples : ils ne servent qu'à montrer.
"####;

const EN: &str = r####"This page is written in NøteCraft. Every block is an example of what you can do, and you can change anything or delete the page whenever you like. It is also an ordinary Markdown file in your folder.

> [!NOTE]
> 💡 Click anywhere and write. Type **/** to add a block, **⌘P** to find a page, **⌘,** for settings.

<!--toc-->

## Write

Text with **bold**, *italic*, <u>underline</u>, ~~strikethrough~~, `inline code` and a [link](https://github.com/yanickimhof/notecraft). The usual shortcuts (⌘B, ⌘I, ⌘U, ⌘E) and Markdown work everywhere. Select a word to change its colour.

- A bulleted list
  - with sub levels
- A second idea

1. A numbered list
2. That numbers itself

- [x] A finished task
- [ ] A task to do

> A quote, to put an idea forward.

A coloured paragraph, to catch the eye. <!--c:red-->

A paragraph on a coloured background. <!--c:bg-yellow-->

---

<details><summary>A toggle block</summary>

The content hides or shows with a click. Handy for details, FAQs or long lists.

</details>

> [!NOTE]
> ⚠️ A callout for a warning. They come in every colour, with the icon you like.

```rust
fn main() {
    println!("Code, with its language shown");
}
```

## Lay it out

<!--columns-->

### On the left

Drag a block to the edge of another to make columns. Each column is independent.

<!--column-->

### In the middle

Columns stack by themselves on a phone, once the page is published.

<!--column-->

### On the right

Useful to compare, present or give a page some air.

<!--/columns-->

| Feature | In the app | On the published page |
|---|---|---|
| Rich text | Yes | Yes |
| Columns | Yes | Yes |
| Images | Yes, resizable | Yes |
| Dark mode | Yes | Yes, automatic |

![The NøteCraft logo](@LOGO@)<!--w:25-->

Drag the round handle on an image corner to resize it, right click to align it left, centre or right. Drop a file on the page to attach it.

<!--bookmark:https://github.com/yanickimhof/notecraft NøteCraft on GitHub-->

## Connect your ideas

- **Pages inside pages**: see [[A page inside a page]], in the tree on the left. Drag them to organise.
- **Links**: type `[[` then a page name. The other page shows a backlink.
- **Graph**: the "Graph" entry in the sidebar shows the links between your pages.
- **Icons, covers, favourites**: in the page menu (•••). The trash keeps everything and restores it.
- **Tabs**: ⌘T opens a tab, ⌘-click on a page opens it in a new tab. They come back when you restart.
- **Reminders**: on a page, menu (•••), Reminder. The "Reminders" entry lists them all.

## Study and visualise

An equation, solved step by step (type `/equation`):

$$
x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
$$

A diagram drawn from text (type `/mermaid`):

```mermaid
graph LR
  A[Idea] --> B[Note] --> C[Structured page] --> D[Publication]
```

```mermaid
sequenceDiagram
  You->>NøteCraft: An idea
  NøteCraft->>You: A tidy page
```

```mermaid
pie title Where the time goes
  "Writing" : 45
  "Organising" : 30
  "Reviewing" : 25
```

## Databases

A database can live inside a page, with Table, Dashboard, Board, List, Calendar, Gallery, Chart and Timeline views. Every row is also a full page. Try switching views:

<!--database:Projects-->

Properties: text, number, select, multi-select, status, date, checkbox, link, email, phone, relation, person, files, unique ID, created and edited time, formula and rollup. Filters, sorting, grouping and row templates are there too.

## Find

⌘P opens the quick finder. Full text search covers every page and database and answers as you type.

## Keep it safe

- **Private pages**: lock a page with a passphrase (strong encryption). A lost passphrase cannot be recovered.
- **Read-only lock**: for pages you do not want to change by accident.
- **Automatic backups** with restore points, and a **version history** for every page.
- **Safe saving**: a sync mishap never silently overwrites your work, a "conflicted copy" is kept.

## Import and export

- **Import** from Evernote, Obsidian, Bear, Joplin, Word, HTML, CSV and Markdown (sidebar, Import). A report lists what was skipped.
- **Export** a page as Markdown, PDF, Word or HTML, or the whole workspace as Markdown.

## Share

- **Presentation** (⌥⌘P): the page opens in your browser, exactly as it will be published. Scroll, zoom, full screen with F.
- **Publish** (••• menu): the page goes to a private link nobody can guess, for free, on your own Cloudflare account. **Preview** first, then **Update**, and **Unpublish** whenever you like. Set it up once in Settings, Publishing.

## Work with AI

- **⌘J**: summarise, continue, improve, translate or ask a question about the page. You see the result before anything changes.
- **Ollama** (everything stays on your Mac) or your own **Claude**, **ChatGPT** or **Grok** key. Pick the engine and model in Settings.
- **MCP**: Claude Desktop and Claude Code can read and edit your notes, with the permissions you choose (read only, read and write, deletion) and, if you want, limited to one page and its sub pages.

## Meetings

Type `/meeting` and start recording: the microphone, the Mac's sound (Teams, Zoom, Meet) or both. Speech is transcribed on your Mac, speakers are told apart and can be renamed, and the AI writes the notes with a template of your choice (summary, minutes, one to one, sales call, lecture…). A custom vocabulary helps spell proper names right.

## Shortcuts to remember

| Keys | Action |
|---|---|
| ⌘P | Find a page |
| ⌘N | New page |
| ⌘T, ⌘W | New tab, close |
| ⌥⌘P | Present |
| ⌘J | AI assistant |
| ⌘/ | Turn the block into |
| ⌘D | Duplicate the block |
| ⌘⇧L | Light or dark |
| ⌘, | Settings |

## At home

Your notes are ordinary files in the folder you picked: back it up like any folder, or keep it in iCloud Drive, Dropbox or kDrive for several Macs. No account, no tracking, no subscription. A `notecraft` command line can search, import, export and back up. You can delete this page and its examples: they are only here to show.
"####;

/// Put the tour in an empty workspace. Does nothing if the workspace already has pages.
pub fn populate(ws: &mut Workspace, fr: bool) -> Result<()> {
    if !ws.pages.is_empty() || !ws.dbs.is_empty() {
        return Ok(());
    }
    let (title, sub_title, db_title, tour_icon) = if fr {
        ("Découvrir NøteCraft", "Une page dans la page", "Projets", "🧭")
    } else {
        ("Discover NøteCraft", "A page inside a page", "Projects", "🧭")
    };
    let home = ws.create_page(None, title)?;
    let sub = ws.create_page(Some(&home), sub_title)?;
    {
        let p = ws.pages.get_mut(&sub).unwrap();
        p.meta.icon = "📄".into();
        p.blocks = vec![
            Block::paragraph(if fr {
                "Les pages s'emboîtent comme des dossiers. Cette page a un rétrolien vers la page de découverte : ouvre le menu de la page pour le voir."
            } else {
                "Pages nest like folders. This page has a backlink to the discovery page: open the page menu to see it."
            }),
            Block::new(
                BlockKind::Bulleted,
                if fr { "Retour : [[Découvrir NøteCraft]]" } else { "Back: [[Discover NøteCraft]]" },
            ),
        ];
    }
    ws.save_page(&sub)?;

    let logo = ws.add_asset_bytes("notecraft-logo.png", LOGO)?;
    let md = if fr { FR } else { EN }.replace("@LOGO@", &logo);
    {
        let p = ws.pages.get_mut(&home).unwrap();
        p.meta.icon = tour_icon.into();
        p.meta.cover = "gradient:1".into();
        p.blocks = from_markdown(&md);
    }
    ws.save_page(&home)?;

    // the database shown in the page, with every kind of view
    let db = ws.create_database(None, db_title)?;
    {
        let d = ws.dbs.get_mut(&db).unwrap();
        d.schema.icon = "🗂".into();
        for (name, kind) in [
            ("Dashboard", ViewKind::Dashboard),
            ("Board", ViewKind::Board),
            ("List", ViewKind::List),
            ("Calendar", ViewKind::Calendar),
            ("Gallery", ViewKind::Gallery),
            ("Chart", ViewKind::Chart),
            ("Timeline", ViewKind::Timeline),
        ] {
            d.schema.views.push(View::new(name, kind));
        }
        d.schema.properties.push(crate::database::Property::new("Done", PropKind::Checkbox));
    }
    ws.save_schema(&db)?;
    let props = ws.dbs[&db].schema.properties.clone();
    let prop = |n: &str| props.iter().find(|p| p.name == n).cloned();
    let day = |offset: i64| -> String {
        let today = jiff::Zoned::now().date();
        today.checked_add(jiff::Span::new().days(offset)).unwrap_or(today).strftime("%Y-%m-%d").to_string()
    };
    let rows: [(&str, &str, &str, i64); 4] = if fr {
        [
            ("Écrire mes premières notes", "Done", "Perso", -2),
            ("Importer mes anciennes notes", "In progress", "Perso", 2),
            ("Publier une page à un ami", "Not started", "Travail", 5),
            ("Essayer l'assistant IA", "Not started", "Idée", 8),
        ]
    } else {
        [
            ("Write my first notes", "Done", "Personal", -2),
            ("Import my old notes", "In progress", "Personal", 2),
            ("Publish a page for a friend", "Not started", "Work", 5),
            ("Try the AI assistant", "Not started", "Idea", 8),
        ]
    };
    for (name, status, tag, off) in rows {
        let id = ws.add_row(&db, None, name)?;
        let r = ws.dbs.get_mut(&db).unwrap().rows.get_mut(&id).unwrap();
        if let Some(p) = prop("Status") {
            r.set_value(&p, PropValue::Select(status.into()));
        }
        if let Some(p) = prop("Tags") {
            r.set_value(&p, PropValue::Multi(vec![tag.to_string()]));
        }
        if let Some(p) = prop("Date") {
            r.set_value(&p, PropValue::Date(day(off)));
        }
        if let Some(p) = prop("Done") {
            r.set_value(&p, PropValue::Checkbox(status == "Done"));
        }
        ws.save_row(&db, &id)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tour_is_made_once_in_both_languages() {
        for fr in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let mut ws = Workspace::open(dir.path()).unwrap();
            populate(&mut ws, fr).unwrap();
            assert_eq!(ws.pages.len(), 2);
            assert_eq!(ws.dbs.len(), 1);
            populate(&mut ws, fr).unwrap();
            assert_eq!(ws.pages.len(), 2);
            let home = ws.children(None).iter().find_map(|t| match &t.doc {
                crate::workspace::DocId::Page(id) if ws.pages[id].meta.cover.starts_with("gradient") => {
                    Some(id.clone())
                }
                _ => None,
            });
            let home = home.expect("tour page");
            let blocks = &ws.pages[&home].blocks;
            assert!(blocks.len() > 60, "{}", blocks.len());
            let has = |f: fn(&BlockKind) -> bool| blocks.iter().any(|b| f(&b.kind));
            assert!(has(|k| matches!(k, BlockKind::Toc)));
            assert!(has(|k| matches!(k, BlockKind::ColumnsStart)));
            assert!(has(|k| matches!(k, BlockKind::Table)));
            assert!(has(|k| matches!(k, BlockKind::Equation)));
            assert!(has(|k| matches!(k, BlockKind::Database)));
            assert!(has(|k| matches!(k, BlockKind::Bookmark { .. })));
            assert!(has(|k| matches!(k, BlockKind::Toggle { .. })));
            assert!(has(|k| matches!(k, BlockKind::Callout { .. })));
            assert!(has(|k| matches!(k, BlockKind::Code { lang } if lang == "mermaid")));
            assert!(blocks
                .iter()
                .any(|b| matches!(&b.kind, BlockKind::Image { path } if ws.root.join(path).is_file())));
            assert!(!ws
                .backlinks(
                    &ws.resolve_title(if fr { "Une page dans la page" } else { "A page inside a page" }).unwrap()
                )
                .is_empty());
        }
    }
}
