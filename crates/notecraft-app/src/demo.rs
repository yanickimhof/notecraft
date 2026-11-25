//! The built in demo: a short slide show of what NøteCraft does, in the language of the app.
//! It is written to a temporary folder and opened in the browser. Nothing is sent anywhere.

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
    ("Le principe", "Vos notes restent à vous", &["De simples fichiers Markdown dans un dossier que vous choisissez", "Sans compte, sans suivi, sans abonnement", "Application native, rapide, même hors ligne"], ""),
    ("Écrire", "Un éditeur par blocs", &["Titres, listes, tâches, citations, encadrés, code", "Colonnes, tableaux, images redimensionnables", "Pages dans les pages, liens et rétroliens", "Onglets, mode sombre, raccourcis partout"], "page.jpg"),
    ("Organiser", "Des bases de données vivantes", &["Tableau, kanban, liste, calendrier, galerie, graphique, frise", "Formules, cumuls et relations entre bases", "Chaque ligne est une vraie page"], "board.jpg"),
    ("Réunions", "Enregistrez, transcrivez, résumez", &["La transcription se fait sur votre Mac", "Intervenants distingués et renommables", "Compte rendu rédigé par l'IA de votre choix"], "meeting.jpg"),
    ("Intelligence artificielle", "L'IA, à votre façon", &["Ollama en local, ou votre propre clé Claude, ChatGPT ou Grok", "Vous voyez toujours le résultat avant tout changement", "Claude Desktop et Claude Code travaillent sur vos notes via MCP"], "ai.jpg"),
    ("Partager", "Publiez en un clic", &["Lien privé impossible à deviner, gratuit, sur votre compte Cloudflare", "Aperçu, mise à jour et retrait quand vous voulez", "Schémas, équations et mode sombre inclus"], "page-dark.jpg"),
];

const EN: &[Slide] = &[
    ("The idea", "Your notes stay yours", &["Plain Markdown files in a folder you choose", "No account, no tracking, no subscription", "Native, fast, works offline"], ""),
    ("Write", "A block editor", &["Headings, lists, tasks, quotes, callouts, code", "Columns, tables, resizable images", "Nested pages, links and backlinks", "Tabs, dark mode, shortcuts everywhere"], "page.jpg"),
    ("Organise", "Living databases", &["Table, board, list, calendar, gallery, chart, timeline", "Formulas, rollups and relations between databases", "Every row is a real page"], "board.jpg"),
    ("Meetings", "Record, transcribe, summarise", &["Transcription runs on your Mac", "Speakers told apart and renameable", "Minutes written by the AI of your choice"], "meeting.jpg"),
    ("Artificial intelligence", "AI, your way", &["Ollama locally, or your own Claude, ChatGPT or Grok key", "You always see the result before anything changes", "Claude Desktop and Claude Code work on your notes through MCP"], "ai.jpg"),
    ("Share", "Publish in one click", &["Private, unguessable link, free, on your own Cloudflare account", "Preview, update and withdraw whenever you want", "Diagrams, equations and dark mode included"], "page-dark.jpg"),
];

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The whole deck as one HTML page.
pub fn html(lang: Lang) -> String {
    let (slides, tagline, end_k, end_t, end_p, hint) = match lang {
        Lang::Fr => (FR, "Notes, bases de données et idées dans une seule app Mac.<br>Vos fichiers, votre dossier, vos règles.", "Pour essayer", "Gratuit pour tout usage non commercial", "Mac Apple silicon, macOS 11 ou plus récent<br>github.com/yanickimhof/notecraft", "← → pour naviguer · F plein écran"),
        Lang::En => (EN, "Notes, databases and ideas in one Mac app.<br>Your files, your folder, your rules.", "Try it", "Free for any non-commercial use", "Apple silicon Mac, macOS 11 or later<br>github.com/yanickimhof/notecraft", "← → to navigate · F full screen"),
    };
    let mut body = String::new();
    body.push_str(&format!("<section class=\"dark cover\"><img src=\"icon.png\" class=\"logo\" alt=\"\"><h1>NøteCraft</h1><p class=\"lead\">{tagline}</p></section>"));
    for (i, (k, t, bullets, img)) in slides.iter().enumerate() {
        let tone = if i % 2 == 0 { "light" } else { "dark" };
        let li: String = bullets.iter().map(|b| format!("<li>{}</li>", esc(b))).collect();
        let pic = if img.is_empty() { String::new() } else { format!("<div class=\"pic\"><img src=\"{img}\" alt=\"\"></div>") };
        body.push_str(&format!("<section class=\"{tone}\"><div class=\"txt\"><p class=\"k\">{}</p><h2>{}</h2><ul>{li}</ul></div>{pic}</section>", esc(k), esc(t)));
    }
    body.push_str(&format!("<section class=\"dark end\"><p class=\"k\">{end_k}</p><h1 class=\"sm\">{end_t}</h1><p class=\"lead\">{end_p}</p></section>"));
    format!(
        "<!doctype html><html lang=\"{}\"><head><meta charset=\"utf-8\"><title>NøteCraft</title><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><style>{CSS}</style></head><body>{body}<div id=\"hint\">{hint}</div><div id=\"count\"></div><script>{JS}</script></body></html>",
        if lang == Lang::Fr { "fr" } else { "en" }
    )
}

const CSS: &str = "*{box-sizing:border-box}html,body{margin:0;height:100%;overflow:hidden;font-family:-apple-system,'Helvetica Neue',sans-serif}section{position:absolute;inset:0;display:none;align-items:center;gap:6vw;padding:7vw 8vw}section.on{display:flex}section.dark{background:#1b1b1a;color:#f0efed}section.light{background:#f6f5f3;color:#2c2c2b}.txt{flex:0 0 38%}.pic{flex:1;height:100%;display:flex;align-items:center;justify-content:center}.pic img{max-width:100%;max-height:100%;object-fit:contain;border-radius:14px;box-shadow:0 10px 40px #0003}h1{font-size:8vw;margin:0;line-height:1.05}h1.sm{font-size:5.4vw}h2{font-size:4.2vw;margin:0 0 2.4vw;line-height:1.1}ul{margin:0;padding:0;list-style:none}li{font-size:1.9vw;line-height:1.35;margin:0 0 1.3vw}.k{font-size:1.4vw;letter-spacing:.14em;text-transform:uppercase;color:#2783de;font-weight:600;margin:0 0 1.6vw}.dark .k{color:#529cca}.lead{font-size:2.4vw;line-height:1.35;opacity:.8;margin:2vw 0 0}.cover,.end{flex-direction:column;align-items:flex-start;justify-content:center;gap:0}.logo{width:11vw;height:11vw;border-radius:2.4vw;margin-bottom:3vw}#hint{position:fixed;left:2vw;bottom:1.4vw;font-size:12px;opacity:.45;color:#888}#count{position:fixed;right:2vw;bottom:1.4vw;font-size:12px;opacity:.45;color:#888}";

const JS: &str = "var s=[].slice.call(document.querySelectorAll('section')),i=0;function g(n){i=Math.max(0,Math.min(s.length-1,n));s.forEach(function(e,k){e.className=e.className.replace(' on','')+(k==i?' on':'')});document.getElementById('count').textContent=(i+1)+' / '+s.length;location.hash=i+1}function f(){document.fullscreenElement?document.exitFullscreen():document.documentElement.requestFullscreen()}document.addEventListener('keydown',function(e){var k=e.key;if(k=='ArrowRight'||k==' '||k=='PageDown')g(i+1);else if(k=='ArrowLeft'||k=='PageUp')g(i-1);else if(k=='Home')g(0);else if(k=='End')g(s.length-1);else if((k=='f'||k=='F')&&!e.metaKey&&!e.ctrlKey)f()});document.addEventListener('click',function(e){g(e.clientX<innerWidth/4?i-1:i+1)});g((parseInt(location.hash.slice(1))||1)-1);";

/// Write the deck next to its pictures in a temporary folder and open it in the browser.
pub fn open(lang: Lang) -> Result<(), String> {
    let dir = std::env::temp_dir().join("notecraft-demo");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for (name, bytes) in IMAGES {
        std::fs::write(dir.join(name), bytes).map_err(|e| e.to_string())?;
    }
    let path = dir.join(if lang == Lang::Fr { "demo-fr.html" } else { "demo-en.html" });
    std::fs::write(&path, html(lang)).map_err(|e| e.to_string())?;
    crate::app::open_external(&path.display().to_string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_languages_have_the_same_slides() {
        assert_eq!(FR.len(), EN.len());
        for l in [Lang::Fr, Lang::En] {
            let h = html(l);
            assert!(h.matches("<section").count() == FR.len() + 2);
        }
    }
}
