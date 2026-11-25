//! The built in demo: a short slide show of what NøteCraft does, in the language of the app.
//! It is shown in a window inside the app: Next, Back, Skip and "Do not show again".

use crate::i18n::Lang;

const IMAGES: &[(&str, &[u8])] = &[
    ("icon.png", include_bytes!("../assets/demo/icon.png")),
    ("page.jpg", include_bytes!("../assets/demo/page.jpg")),
    ("page-dark.jpg", include_bytes!("../assets/demo/page-dark.jpg")),
    ("board.jpg", include_bytes!("../assets/demo/board.jpg")),
    ("meeting.jpg", include_bytes!("../assets/demo/meeting.jpg")),
    ("ai.jpg", include_bytes!("../assets/demo/ai.jpg")),
];

/// (kicker, title, bullets, image) for each slide after the cover.
type Slide = (&'static str, &'static str, &'static [&'static str], &'static str);

const FR: &[Slide] = &[
    (
        "Le principe",
        "Vos notes restent à vous",
        &[
            "De simples fichiers Markdown dans un dossier que vous choisissez",
            "Sans compte, sans suivi, sans abonnement",
            "Application native, rapide, même hors ligne",
        ],
        "",
    ),
    (
        "Écrire",
        "Un éditeur par blocs",
        &[
            "Titres, listes, tâches, citations, encadrés, code",
            "Colonnes, tableaux, images redimensionnables",
            "Pages dans les pages, liens et rétroliens",
            "Onglets, mode sombre, raccourcis partout",
        ],
        "page.jpg",
    ),
    (
        "Organiser",
        "Des bases de données vivantes",
        &[
            "Tableau, kanban, liste, calendrier, galerie, graphique, frise",
            "Formules, cumuls et relations entre bases",
            "Chaque ligne est une vraie page",
        ],
        "board.jpg",
    ),
    (
        "Réunions",
        "Enregistrez, transcrivez, résumez",
        &[
            "La transcription se fait sur votre Mac",
            "Intervenants distingués et renommables",
            "Compte rendu rédigé par l'IA de votre choix",
        ],
        "meeting.jpg",
    ),
    (
        "Intelligence artificielle",
        "L'IA, à votre façon",
        &[
            "Ollama en local, ou votre propre clé Claude, ChatGPT ou Grok",
            "Vous voyez toujours le résultat avant tout changement",
            "Claude Desktop et Claude Code travaillent sur vos notes via MCP",
        ],
        "ai.jpg",
    ),
    (
        "Partager",
        "Publiez en un clic",
        &[
            "Lien privé impossible à deviner, gratuit, sur votre compte Cloudflare",
            "Aperçu, mise à jour et retrait quand vous voulez",
            "Schémas, équations et mode sombre inclus",
        ],
        "page-dark.jpg",
    ),
];

const EN: &[Slide] = &[
    (
        "The idea",
        "Your notes stay yours",
        &[
            "Plain Markdown files in a folder you choose",
            "No account, no tracking, no subscription",
            "Native, fast, works offline",
        ],
        "",
    ),
    (
        "Write",
        "A block editor",
        &[
            "Headings, lists, tasks, quotes, callouts, code",
            "Columns, tables, resizable images",
            "Nested pages, links and backlinks",
            "Tabs, dark mode, shortcuts everywhere",
        ],
        "page.jpg",
    ),
    (
        "Organise",
        "Living databases",
        &[
            "Table, board, list, calendar, gallery, chart, timeline",
            "Formulas, rollups and relations between databases",
            "Every row is a real page",
        ],
        "board.jpg",
    ),
    (
        "Meetings",
        "Record, transcribe, summarise",
        &[
            "Transcription runs on your Mac",
            "Speakers told apart and renameable",
            "Minutes written by the AI of your choice",
        ],
        "meeting.jpg",
    ),
    (
        "Artificial intelligence",
        "AI, your way",
        &[
            "Ollama locally, or your own Claude, ChatGPT or Grok key",
            "You always see the result before anything changes",
            "Claude Desktop and Claude Code work on your notes through MCP",
        ],
        "ai.jpg",
    ),
    (
        "Share",
        "Publish in one click",
        &[
            "Private, unguessable link, free, on your own Cloudflare account",
            "Preview, update and withdraw whenever you want",
            "Diagrams, equations and dark mode included",
        ],
        "page-dark.jpg",
    ),
];

/// One page of the demo.
pub struct Page {
    pub kicker: String,
    pub title: String,
    pub bullets: Vec<String>,
    pub image: Option<&'static str>,
}

/// The pages in the language of the app: a cover, the feature slides and a closing page.
pub fn pages(lang: Lang) -> Vec<Page> {
    let (slides, tagline, end_k, end_t, end_p) = match lang {
        Lang::Fr => (
            FR,
            "Notes, bases de données et idées dans une seule app Mac. Vos fichiers, votre dossier, vos règles.",
            "Pour essayer",
            "Gratuit pour tout usage non commercial",
            "Mac Apple silicon, macOS 11 ou plus récent. github.com/yanickimhof/notecraft",
        ),
        Lang::En => (
            EN,
            "Notes, databases and ideas in one Mac app. Your files, your folder, your rules.",
            "Try it",
            "Free for any non-commercial use",
            "Apple silicon Mac, macOS 11 or later. github.com/yanickimhof/notecraft",
        ),
    };
    let mut v = vec![Page {
        kicker: String::new(),
        title: "NøteCraft".into(),
        bullets: vec![tagline.into()],
        image: Some("icon.png"),
    }];
    for (k, t, b, img) in slides {
        v.push(Page {
            kicker: (*k).into(),
            title: (*t).into(),
            bullets: b.iter().map(|x| (*x).to_string()).collect(),
            image: if img.is_empty() { None } else { Some(*img) },
        });
    }
    v.push(Page { kicker: end_k.into(), title: end_t.into(), bullets: vec![end_p.into()], image: None });
    v
}

/// The bytes of a demo picture, by name.
pub fn image(name: &str) -> &'static [u8] {
    IMAGES.iter().find(|(n, _)| *n == name).map(|(_, b)| *b).unwrap_or(&[])
}

impl crate::app::NcApp {
    /// The demo window: pictures and a few lines per page, with Back, Next, Skip and "Do not show again".
    /// It opens by itself once the workspace is open, until the person asks not to see it again.
    pub(crate) fn demo_window(&mut self, ctx: &eframe::egui::Context) {
        use eframe::egui::{self, RichText, Vec2};
        if self.demo_at.is_none()
            && !self.demo_auto_done
            && self.ws.is_some()
            && !self.prefs.demo_seen
            && self.shot.is_none()
        {
            self.demo_auto_done = true;
            self.demo_at = Some(0);
        }
        let Some(at) = self.demo_at else { return };
        let pages = pages(self.prefs.lang);
        let at = at.min(pages.len() - 1);
        let last = at + 1 == pages.len();
        let pal = self.pal;
        let (mut next, mut back, mut close) = (false, false, false);
        let mut never = self.prefs.demo_seen;
        let (back_t, next_t, skip_t, done_t, never_t) =
            (self.t("onb_back"), self.t("onb_next"), self.t("onb_skip"), self.t("onb_finish"), self.t("demo_never"));
        let modal = egui::Modal::new(egui::Id::unique("nc-demo")).show(ctx, |ui| {
            ui.set_width(720.0);
            let page = &pages[at];
            // Every page has the same height, so Next stays under the pointer: click, click, click.
            ui.allocate_ui_with_layout(Vec2::new(720.0, 420.0), egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.set_min_size(Vec2::new(720.0, 420.0));
                match page.image {
                    None => ui.add_space(110.0),
                    Some("icon.png") => ui.add_space(50.0),
                    _ => {}
                }
                if let Some(img) = page.image {
                    let (w, h) = if img == "icon.png" { (120.0, 120.0) } else { (680.0, 360.0) };
                    ui.add(
                        egui::Image::from_bytes(format!("bytes://nc-demo-{img}"), image(img))
                            .fit_to_exact_size(Vec2::new(w, h))
                            .corner_radius(12.0),
                    );
                    ui.add_space(14.0);
                }
                if !page.kicker.is_empty() {
                    ui.label(RichText::new(page.kicker.to_uppercase()).size(11.5).strong().color(pal.accent));
                }
                ui.label(RichText::new(&page.title).size(26.0).color(pal.text));
                ui.add_space(8.0);
                for b in &page.bullets {
                    ui.label(RichText::new(b).size(14.5).color(pal.text_secondary));
                    ui.add_space(3.0);
                }
            });
            ui.add_space(14.0);
            ui.label(
                RichText::new((0..pages.len()).map(|i| if i == at { "●" } else { "○" }).collect::<Vec<_>>().join(" "))
                    .size(10.0)
                    .color(pal.text_faint),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut never, never_t);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new(if last { done_t } else { next_t }).size(14.0))
                                .fill(pal.accent_soft),
                        )
                        .clicked()
                    {
                        next = true;
                    }
                    if at > 0 && ui.button(back_t).clicked() {
                        back = true;
                    }
                    if !last && crate::widgets::text_link(ui, &pal, skip_t).clicked() {
                        close = true;
                    }
                });
            });
        });
        if modal.should_close() {
            close = true;
        }
        self.prefs.demo_seen = never;
        if next && last {
            close = true;
            self.prefs.demo_seen = true;
        }
        self.demo_at = if close {
            None
        } else if next {
            Some(at + 1)
        } else if back {
            Some(at - 1)
        } else {
            Some(at)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_languages_have_the_same_pages_and_known_pictures() {
        assert_eq!(FR.len(), EN.len());
        for l in [Lang::Fr, Lang::En] {
            let p = pages(l);
            assert_eq!(p.len(), FR.len() + 2);
            for pg in p {
                if let Some(i) = pg.image {
                    assert!(!image(i).is_empty());
                }
            }
        }
    }
}
