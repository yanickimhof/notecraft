//! Manual check: which source is used and does it deliver sound?
//! Usage: mic_check [microphone name] [mic|system|both]
use cpal::traits::HostTrait;
use notecraft_meeting::{Recorder, Source};

fn main() {
    let (list, default) = notecraft_meeting::input_devices();
    for d in &list {
        println!("input device: {d}");
    }
    println!("default: {default:?}");
    let name = std::env::args().nth(1).filter(|n| !n.is_empty());
    let source = Source::from_id(&std::env::args().nth(2).unwrap_or_default());
    println!("output: {:?}", cpal::default_host().default_output_device().is_some());
    let r = Recorder::start_with(name.as_deref(), source).expect("start");
    let mut loudest = 0f32;
    let mut total = 0usize;
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        loudest = loudest.max(r.level());
        total += r.take().len();
    }
    println!(
        "{:?} on {}: samples in 4 s: {total} (expect ~64000), loudest level: {loudest}, stats {:?}",
        source,
        r.device_name(),
        r.stats()
    );
}
