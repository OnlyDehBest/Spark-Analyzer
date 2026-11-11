use sparkscope::usage::UsageStats;

fn temp_file(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("sparkscope-usage-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("usage.json")
}

#[test]
fn counts_people_once_and_every_analysis() {
    let s = UsageStats::in_memory();
    s.record("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    s.record("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    s.record("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    assert_eq!(s.snapshot(), (2, 3));
}

#[test]
fn totals_survive_a_restart_without_storing_visitor_ids() {
    let file = temp_file("restart");
    {
        let s = UsageStats::load(file.clone(), std::iter::empty());
        s.record("cccccccccccccccccccccccccccccccc");
        s.record("dddddddddddddddddddddddddddddddd");
    }
    let raw = std::fs::read_to_string(&file).unwrap();
    assert!(!raw.contains("cccccccc"), "{raw}");
    let s = UsageStats::load(file.clone(), std::iter::empty());
    assert_eq!(s.snapshot(), (2, 2));
    s.record("cccccccccccccccccccccccccccccccc");
    assert_eq!(s.snapshot(), (2, 3));
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}

#[test]
fn first_start_counts_the_reports_already_saved() {
    let file = temp_file("seed");
    let owners = ["e1", "e1", "e2"];
    let s = UsageStats::load(file.clone(), owners.iter().copied());
    assert_eq!(s.snapshot(), (2, 3));
    let again = UsageStats::load(file.clone(), ["x9"].iter().copied());
    assert_eq!(again.snapshot(), (2, 3));
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}
