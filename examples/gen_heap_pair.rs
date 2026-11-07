#[path = "../tests/support/fixture.rs"]
mod fixture;

use prost::Message;

fn main() {
    let mut args = std::env::args().skip(1);
    let before = args.next().unwrap_or_else(|| "heap-prima.sparkheap".into());
    let after = args.next().unwrap_or_else(|| "heap-dopo.sparkheap".into());
    let (a, b) = fixture::sample_heap_pair();
    std::fs::write(&before, a.encode_to_vec()).expect("scrittura fallita");
    std::fs::write(&after, b.encode_to_vec()).expect("scrittura fallita");
    eprintln!("scritti {before} e {after}");
}
