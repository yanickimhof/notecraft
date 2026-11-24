//! Manual check: download a model (default base), transcribe a 16 kHz mono f32 raw file.
use notecraft_meeting::{models, transcribe::Event, Transcriber, WhisperModel};

fn main() {
    let raw = std::env::args().nth(1).expect("raw f32 file");
    let lang = std::env::args().nth(2).unwrap_or_else(|| "auto".into());
    let model = std::env::args().nth(3).unwrap_or_else(|| "base".into());
    let m = WhisperModel::by_id(&model).unwrap();
    if !m.installed() {
        let mut last = 0;
        models::download(m, |d, t| {
            let pct = d * 100 / t.max(1);
            if pct != last && pct % 20 == 0 {
                eprintln!("download {pct}%");
            }
            last = pct;
            true
        })
        .expect("download");
    }
    let bytes = std::fs::read(raw).unwrap();
    let samples: Vec<f32> = bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect();
    let t0 = std::time::Instant::now();
    let tr = Transcriber::start(&m.path(), &lang);
    tr.push(samples);
    tr.finish();
    loop {
        match tr.events.recv().unwrap() {
            Event::Ready => eprintln!("ready after {:?}", t0.elapsed()),
            Event::Text(t) => println!("TEXT: {t}"),
            Event::Done => break,
            Event::Failed(e) => panic!("{e}"),
        }
    }
    eprintln!("total {:?}", t0.elapsed());
}
