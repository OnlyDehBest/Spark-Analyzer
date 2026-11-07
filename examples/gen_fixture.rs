#[path = "../tests/support/fixture.rs"]
mod fixture;

use prost::Message;

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sample.sparkprofile".into());
    let alloc = std::env::args().any(|a| a == "--alloc");
    let heap = std::env::args().any(|a| a == "--heap");
    let bytes = if heap {
        fixture::sample_heap().encode_to_vec()
    } else if alloc {
        fixture::sample_alloc().encode_to_vec()
    } else {
        fixture::sample().encode_to_vec()
    };
    std::fs::write(&out, bytes).expect("scrittura fallita");
    eprintln!("scritto {out}");
}
