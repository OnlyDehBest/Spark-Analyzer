use std::time::Duration;

use sparkscope::archive::{Archive, ArchivedReport};

const DAY: u64 = 24 * 3600;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sparkscope-archive-{tag}-{}-{}",
        std::process::id(),
        sparkscope::archive::now_secs()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn meta(id: &str, created: u64) -> ArchivedReport {
    ArchivedReport {
        id: id.into(),
        owner: "0123456789abcdef0123456789abcdef".into(),
        title: format!("Report {id}"),
        detail: "Paper 1.21.4".into(),
        bad: true,
        name: "server.sparkprofile".into(),
        key: None,
        second_name: None,
        second_key: None,
        created,
    }
}

#[test]
fn saved_reports_come_back_after_a_restart_newest_first() {
    let dir = temp_dir("restart");
    let now = 100 * DAY;
    {
        let a = Archive::open(dir.clone(), Duration::from_secs(30 * DAY)).unwrap();
        a.save(&meta("aa01", now - 2 * DAY), b"first profile", None)
            .unwrap();
        a.save(&meta("aa02", now - DAY), b"heap a", Some(b"heap b"))
            .unwrap();
    }
    let a = Archive::open(dir.clone(), Duration::from_secs(30 * DAY)).unwrap();
    let all = a.load_all(now);
    assert_eq!(
        all.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["aa02", "aa01"]
    );
    assert_eq!(
        a.payload("aa01").unwrap(),
        (b"first profile".to_vec(), None)
    );
    assert_eq!(
        a.payload("aa02").unwrap(),
        (b"heap a".to_vec(), Some(b"heap b".to_vec()))
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn reports_older_than_the_retention_are_deleted_from_disk() {
    let dir = temp_dir("prune");
    let now = 100 * DAY;
    let a = Archive::open(dir.clone(), Duration::from_secs(30 * DAY)).unwrap();
    a.save(&meta("bb01", now - 31 * DAY), b"old", Some(b"old b"))
        .unwrap();
    a.save(&meta("bb02", now - 29 * DAY), b"recent", None)
        .unwrap();
    assert_eq!(a.load_all(now).len(), 1);
    assert_eq!(a.prune(now), ["bb01"]);
    let left: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(left.iter().all(|f| f.starts_with("bb02")), "{left:?}");
    assert!(a.payload("bb01").is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn updated_share_keys_are_kept() {
    let dir = temp_dir("update");
    let a = Archive::open(dir.clone(), Duration::from_secs(30 * DAY)).unwrap();
    let mut m = meta("cc01", 10);
    a.save(&m, b"x", None).unwrap();
    m.key = Some("AbCd1234".into());
    a.update(&m).unwrap();
    assert_eq!(a.load_all(20)[0].key.as_deref(), Some("AbCd1234"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unsafe_ids_and_broken_files_are_ignored() {
    let dir = temp_dir("unsafe");
    let a = Archive::open(dir.clone(), Duration::from_secs(30 * DAY)).unwrap();
    assert!(a.save(&meta("../etc/passwd", 1), b"x", None).is_err());
    assert!(a.payload("../../x").is_err());
    std::fs::write(dir.join("dd01.json"), b"{not json").unwrap();
    a.save(&meta("dd02", 5), b"ok", None).unwrap();
    assert_eq!(a.load_all(6).len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}
