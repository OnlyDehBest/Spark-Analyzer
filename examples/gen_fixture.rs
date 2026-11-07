#[path = "../tests/support/fixture.rs"]
mod fixture;

use prost::Message;

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sample.sparkprofile".into());
    let alloc = std::env::args().any(|a| a == "--alloc");
    let data = if alloc {
        fixture::sample_alloc()
    } else {
        fixture::sample()
    };
    std::fs::write(&out, data.encode_to_vec()).expect("scrittura fallita");
    eprintln!("scritto {out}");
}
