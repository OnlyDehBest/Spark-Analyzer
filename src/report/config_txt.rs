use std::fmt::Write;

use super::{Report, fmt_duration};
use crate::rules::ConfigChange;

const FILE_ORDER: &[&str] = &[
    "server.properties",
    "bukkit.yml",
    "spigot.yml",
    "config/paper-global.yml",
    "config/paper-world-defaults.yml",
    "purpur.yml",
    "pufferfish.yml",
];

const RULE_WIDTH: usize = 64;

pub fn file_rank(file: &str) -> usize {
    FILE_ORDER
        .iter()
        .position(|f| *f == file)
        .unwrap_or(FILE_ORDER.len())
}

pub fn file_name() -> &'static str {
    crate::i18n::t("configurazioni-ottimizzate.txt", "optimized-settings.txt")
}

fn severity_word(c: &ConfigChange) -> String {
    c.severity.label().to_lowercase()
}

fn write_change(o: &mut String, c: &ConfigChange) {
    let t = crate::i18n::t;
    let warn = if c.gameplay { " [!]" } else { "" };
    let _ = writeln!(o, "{}", c.path);
    let _ = writeln!(
        o,
        "    {} {}",
        t("attuale:", "current:"),
        c.current
            .as_deref()
            .unwrap_or(t("non presente nel profilo", "not in the profile"))
    );
    let _ = writeln!(o, "    {}   {}{warn}", t("nuovo:", "new:  "), c.recommended);
    let _ = writeln!(
        o,
        "    {}  {} ({})",
        t("perché:", "why:   "),
        c.reason,
        severity_word(c)
    );
    if !c.note.is_empty() {
        let _ = writeln!(o, "    {}    {}", t("nota:", "note:"), c.note);
    }
    let _ = writeln!(o);
}

pub fn render(r: &Report) -> String {
    let t = crate::i18n::t;
    let m = &r.meta;
    let mut o = String::with_capacity(512 + r.config_changes.len() * 256);
    let _ = writeln!(
        o,
        "SPARK RESOURCE ANALYZER — {}",
        t("CONFIGURAZIONI DA OTTIMIZZARE", "SETTINGS TO OPTIMIZE")
    );
    let _ = writeln!(o, "{}", "=".repeat(RULE_WIDTH));
    if !m.platform.is_empty() {
        let mc = if m.minecraft_version.is_empty() {
            String::new()
        } else {
            format!(" (Minecraft {})", m.minecraft_version)
        };
        let _ = writeln!(o, "{:<11}{}{mc}", t("Server:", "Server:"), m.platform);
    }
    let mut profile = Vec::with_capacity(3);
    if let Some(v) = &m.viewer_url {
        profile.push(v.clone());
    }
    if m.duration_s > 0.0 {
        profile.push(fmt_duration(m.duration_s));
    }
    if m.players > 0 {
        profile.push(tf!("{} giocatori", "{} players", m.players));
    }
    if !profile.is_empty() {
        let _ = writeln!(
            o,
            "{:<11}{}",
            t("Profilo:", "Profile:"),
            profile.join("  ·  ")
        );
    }
    let _ = writeln!(o);
    let _ = writeln!(
        o,
        "{}",
        tf!(
            "{} impostazioni da cambiare, in ordine di importanza dentro ogni file.",
            "{} settings to change, most important first within each file.",
            r.config_changes.len()
        )
    );
    let _ = writeln!(
        o,
        "{}",
        t(
            "Prima di modificare fai un backup dei file, poi riavvia il server:\nspigot.yml, bukkit.yml e i file di Paper non si ricaricano da soli.",
            "Back up the files before editing, then restart the server:\nspigot.yml, bukkit.yml and the Paper files are not reloaded on their own."
        )
    );
    let _ = writeln!(
        o,
        "{}\n",
        t(
            "Le righe con [!] cambiano il gameplay: leggi la nota prima di applicarle.",
            "Lines marked [!] change gameplay: read the note before applying them."
        )
    );
    let mut current_file: Option<&str> = None;
    for c in &r.config_changes {
        if current_file != Some(c.file.as_str()) {
            current_file = Some(&c.file);
            let _ = writeln!(o, "{}", "-".repeat(RULE_WIDTH));
            let _ = writeln!(o, "{}", c.file);
            let _ = writeln!(o, "{}", "-".repeat(RULE_WIDTH));
        }
        write_change(&mut o, c);
    }
    let _ = writeln!(o, "{}", "-".repeat(RULE_WIDTH));
    let _ = writeln!(
        o,
        "{}",
        t(
            "Dopo i cambi registra un nuovo profilo (/spark profiler start --timeout 300)\ne confrontalo con questo per vedere quanto hai guadagnato.",
            "After the changes record a new profile (/spark profiler start --timeout 300)\nand compare it with this one to see how much you gained."
        )
    );
    let _ = writeln!(
        o,
        "© {} {} · Spark Resource Analyzer",
        super::html::COPYRIGHT_YEAR,
        super::html::COPYRIGHT_OWNER
    );
    o
}

pub fn data_uri(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2 + 32);
    out.push_str("data:text/plain;charset=utf-8,");
    for b in text.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}
