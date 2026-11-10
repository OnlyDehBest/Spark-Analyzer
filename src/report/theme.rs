pub const FONTS: &str = r#"<link rel="preconnect" href="https://fonts.googleapis.com"><link rel="preconnect" href="https://fonts.gstatic.com" crossorigin><link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500&family=IBM+Plex+Sans:wght@400;500;600;700&display=swap">"#;

pub const BRAND_CSS: &str = r#"
:root,[data-theme=dark]{--bg:#0f1217;--side:#0f1217;--card:#151a21;--card2:#1a2029;--card3:#232a35;--line:#272f3b;--line2:#1e252f;--fg:#c3ccd8;--hd:#f3f6fa;--muted:#8592a5;--acc:#3b82f6;--acc2:#2f6fdc;--accl:rgba(59,130,246,.14);--crit:#f05252;--high:#f08a3c;--med:#e3b341;--low:#58a6ff;--info:#8592a5;--ok:#3fb68b;--bar:#3b82f6;--spike:rgba(240,82,82,.10);--code:#1a2029;--glass:rgba(15,18,23,.86);--focus:#7cb1ff}
[data-theme=light]{--bg:#f4f6f9;--side:#ffffff;--card:#ffffff;--card2:#f8fafc;--card3:#eef2f7;--line:#e1e7ef;--line2:#edf1f6;--fg:#334155;--hd:#0f172a;--muted:#64748b;--acc:#2563eb;--acc2:#1d4ed8;--accl:rgba(37,99,235,.08);--crit:#dc2626;--high:#ea580c;--med:#b7791f;--low:#2563eb;--info:#64748b;--ok:#059669;--bar:#2563eb;--spike:rgba(220,38,38,.07);--code:#f1f5f9;--glass:rgba(244,246,249,.88);--focus:#2563eb}
body{font-family:"IBM Plex Sans",ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,Helvetica,Arial,sans-serif;font-size:14px;line-height:1.55;letter-spacing:0}
td,.sv,.kpi,.score b,.cstats,.chrono dd,.chrono .t,.chrono .q,.ritem,.tn .tv{font-variant-numeric:tabular-nums}
code,kbd,pre,.mono,.tn .tl,.cmds code,.chrono code,.chrono .t{font-family:"IBM Plex Mono",ui-monospace,SFMono-Regular,Menlo,Consolas,monospace}
th,.sv small,.sl,.legend,.chart .axis,.ctx,.ritem div span,h5,.seg button,.sev,.area,.pick .fmt,.chrono-meta,.chrono .nth,.chrono .pill,.chrono dd,.chrono .why .q,.chrono table.dt .n,.chrono .tp,.chrono .ms{font-family:inherit;font-variant-numeric:tabular-nums}
:focus-visible{outline:2px solid var(--focus);outline-offset:2px;border-radius:4px}
h1{font-size:30px;font-weight:600;letter-spacing:-.015em;line-height:1.2}
h2{font-size:16px;font-weight:600;letter-spacing:-.005em}
h3{font-size:14px;font-weight:600}
h5,th,.area,.chrono dt,.chrono table.dt th{text-transform:none;letter-spacing:0}
h5{font-size:12px;font-weight:600;color:var(--muted)}
th{font-size:12px;font-weight:600;color:var(--muted)}
.sub{font-size:13px;color:var(--muted)}
.panel{border-radius:10px;padding:18px 20px}
.side{border-radius:10px}
.side nav a{border-radius:6px}
.side nav a.on{background:var(--accl);color:var(--hd);box-shadow:inset 2px 0 0 var(--acc)}
.btn{border-radius:6px;font-weight:600;background:var(--acc);color:#fff;border:1px solid transparent}.btn:hover{background:var(--acc2);color:#fff}.btn.sec{background:var(--card);color:var(--hd);border:1px solid var(--line)}.btn.sec:hover{background:var(--card2);border-color:var(--muted);color:var(--hd)}
.ghost{border-radius:6px}
.stat,.kpi{animation:none!important}
.stat:hover,.kpi:hover{transform:none!important;box-shadow:none!important}
.stat .si,.stat:hover .si{transition:none;transform:none}
.finding summary:hover{transform:none!important}
.finding.crit .sev{animation:none!important}
.sev{display:inline-flex;align-items:center;gap:6px;background:none!important;padding:0;border-radius:0;font-size:12px;font-weight:600;letter-spacing:0;text-transform:none;color:var(--muted);min-width:64px}
.sev::before{content:"";width:8px;height:8px;border-radius:50%;background:currentColor;flex:none}
.finding{background:transparent;border:0;border-bottom:1px solid var(--line2);border-left:0!important;border-radius:0;margin:0}
.finding:last-of-type{border-bottom:0}
.finding summary{padding:12px 6px}
.finding summary:hover{background:var(--card2)}
.finding.crit .sev{color:var(--crit)}.finding.high .sev{color:var(--high)}.finding.med .sev{color:var(--med)}.finding.low .sev{color:var(--low)}
.area{background:none;border:0;padding:0;font-size:12px;color:var(--muted)}
.chip{border-radius:6px}
.seg{border-radius:7px}
.seg button{font-weight:600}
.sumcard{border-left-width:1px;border-radius:10px;padding:18px 20px}
.sumcard .sumtxt .l1{font-size:15px}
.callout{border-left-width:2px;border-radius:8px}
.chart{border-radius:10px}
.news{border-radius:8px}.news .nb{color:var(--acc);background:var(--accl);text-transform:none;letter-spacing:0;font-size:12px;border-radius:5px;padding:4px 7px}
.hcard{background:var(--card)!important;border-radius:12px;animation:none}
.hcard .wave{opacity:.35}
.hcard .wave polyline{stroke:var(--acc)}
.hcard h1{font-size:40px;letter-spacing:-.025em}
.chips span{border-radius:6px}
.hero,.drop{border-radius:12px;border-width:1px;border-style:dashed}
.chrono .pill{text-transform:none;letter-spacing:0;font-size:12px;font-weight:600;padding:2px 8px}
.chrono dt{font-size:12px;font-weight:500}
.chrono dd{font-size:15px;font-weight:600}
.chrono .why .lbl{text-transform:none;letter-spacing:0;font-size:12px;font-weight:500}
.chrono .why .q{font-size:12.5px}
.chrono-meta{text-transform:none;letter-spacing:0;font-size:12px}
.chrono table.dt th{font-size:12px}
.chrono table.dt .n{font-size:13px}
.cstats,.kpis{border-radius:10px}
.cstats .cs span{text-transform:none;letter-spacing:0;font-size:12px}
.cstats .cs b{font:600 20px/1.3 "IBM Plex Sans",system-ui,sans-serif;font-variant-numeric:tabular-nums}
.kpi .sv,.cstats .cs b{letter-spacing:-.01em}
.chrono .t{font-size:12px;font-weight:500}
@media (prefers-reduced-motion:reduce){*,*::before,*::after{animation:none!important;transition:none!important;scroll-behavior:auto!important}}
"#;
