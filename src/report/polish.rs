pub const POLISH_CSS: &str = r#"
[data-theme=dark]{--bg:#0b0b0c;--side:#0f0f10;--card:#131314;--card2:#18181a;--card3:#222225;--line:#25252a;--line2:#1b1b1f}
[data-theme=dark] body{background:var(--bg)}
[data-theme=dark] .content{position:relative;isolation:isolate}
[data-theme=dark] .content::before{content:"";position:fixed;inset:0;z-index:-1;pointer-events:none;background:radial-gradient(900px 520px at 78% -12%,rgba(56,189,248,.11),transparent 62%),radial-gradient(760px 460px at 22% -10%,rgba(163,230,53,.08),transparent 62%)}
.side{border-radius:16px;box-shadow:inset 0 1px 0 rgba(255,255,255,.03)}
[data-theme=dark] .side{background:linear-gradient(180deg,#121214,#0e0e10)}
.brand{font-weight:800;letter-spacing:-.01em}
.side nav a{border-radius:9px;transition:background-color .15s ease,color .15s ease}
.side nav a.on{background:linear-gradient(90deg,rgba(56,189,248,.18),rgba(56,189,248,.03));color:var(--hd);box-shadow:inset 2px 0 0 var(--acc)}
.sidefoot{border-radius:12px}
.cmds code{border-radius:8px;transition:border-color .15s ease}
.cmds code:hover{border-color:var(--acc)}
.news{border-radius:999px;box-shadow:0 12px 30px -18px rgba(163,230,53,.45)}
[data-theme=dark] .news{background:rgba(19,19,20,.85);border-color:rgba(163,230,53,.22)}
.news .nb{box-shadow:0 0 14px rgba(163,230,53,.35)}
.hcard{position:relative;border-radius:22px;padding:42px 40px 34px;border-color:transparent}
[data-theme=dark] .hcard{background:radial-gradient(120% 150% at 0% 0%,rgba(163,230,53,.13),transparent 48%),radial-gradient(90% 130% at 100% 100%,rgba(56,189,248,.15),transparent 55%),linear-gradient(180deg,#161618,#111113)!important;box-shadow:0 40px 90px -50px rgba(0,0,0,.95)}
.hcard::after{content:"";position:absolute;inset:0;border-radius:inherit;padding:1px;pointer-events:none;background:linear-gradient(135deg,rgba(163,230,53,.55),rgba(255,255,255,.06) 38%,rgba(255,255,255,.04) 62%,rgba(56,189,248,.55));-webkit-mask:linear-gradient(#000 0 0) content-box,linear-gradient(#000 0 0);-webkit-mask-composite:xor;mask-composite:exclude}
.hcard .hlogo{width:68px;height:68px;filter:drop-shadow(0 12px 30px rgba(163,230,53,.28))}
.hcard h1{font-size:56px;font-weight:800;letter-spacing:-.04em;line-height:1}
[data-theme=dark] .hcard h1{background:linear-gradient(180deg,#ffffff 30%,#9a9a9a);-webkit-background-clip:text;background-clip:text;color:transparent}
.hcard .wave{opacity:.9}
.hcard .wave polyline{stroke-width:2.6;filter:drop-shadow(0 0 7px rgba(163,230,53,.55))}
.htag{font-size:17px;line-height:1.6;max-width:700px;margin-top:18px}
[data-theme=dark] .htag{color:#bcbcbc}
.chips{gap:10px;margin-top:20px}
.chips span{padding:6px 13px;font-size:12.5px;font-weight:500;border-radius:999px}
[data-theme=dark] .chips span{background:rgba(255,255,255,.035);border-color:rgba(255,255,255,.09);color:#cfcfcf}
.seg.big{padding:4px;border-radius:14px;gap:4px}
[data-theme=dark] .seg.big{background:rgba(19,19,20,.85);border-color:rgba(255,255,255,.07)}
.seg.big button{border-radius:10px;border:0!important;transition:background-color .15s ease,color .15s ease}
.seg.big button.on{color:var(--hd);box-shadow:0 1px 0 rgba(255,255,255,.06) inset,0 8px 18px -10px rgba(0,0,0,.9)}
[data-theme=dark] .seg.big button.on{background:linear-gradient(180deg,#27272b,#1e1e21)}
.hero{border-radius:22px;border-style:solid;border-width:1px;padding:46px 32px 34px;transition:border-color .2s ease,box-shadow .2s ease}
[data-theme=dark] .hero{background:linear-gradient(180deg,rgba(23,23,25,.86),rgba(16,16,18,.86));border-color:rgba(255,255,255,.08);box-shadow:0 40px 90px -50px rgba(0,0,0,.95)}
.hero:hover,.hero.over{border-color:rgba(56,189,248,.55);box-shadow:0 0 0 4px rgba(56,189,248,.08),0 40px 90px -50px rgba(0,0,0,.95)}
.hero .dicon{width:58px;height:58px;border-radius:17px;background:linear-gradient(135deg,#5ccbfa,#0ea5e9);color:#04131c;box-shadow:0 16px 34px -12px rgba(56,189,248,.75)}
.hero h2{font-size:22px;font-weight:700;letter-spacing:-.02em;margin-top:18px}
.lrow{gap:10px}
.lrow input{height:52px;border-radius:13px;padding:0 18px;font-size:15px;transition:border-color .15s ease,box-shadow .15s ease}
[data-theme=dark] .lrow input{background:#0d0d0e;border-color:#2a2a2f}
.lrow input:focus{border-color:var(--acc);box-shadow:0 0 0 4px rgba(56,189,248,.15)}
#lform .btn,.slinkform .btn{height:52px;border-radius:13px;padding:0 24px;background:var(--acc);color:#04131c;border:0;font-weight:700;box-shadow:0 12px 26px -12px rgba(56,189,248,.8)}
#lform .btn:hover,.slinkform .btn:hover{background:#6fd0fb}
.or{margin:22px auto}
.pick{min-height:50px;border-radius:13px;padding:0 18px;transition:border-color .15s ease,background-color .15s ease}
.pick:hover{border-color:var(--acc)!important}
.pick .fmt i{box-shadow:0 0 12px rgba(234,179,8,.25)}
.hero .hint{margin-top:18px;font-size:12.5px}
.busy{border-radius:16px}
.panel{border-radius:18px}
[data-theme=dark] .panel{background:linear-gradient(180deg,#141416,#111113);box-shadow:0 30px 70px -45px rgba(0,0,0,.95)}
.sumcard{border-radius:18px;padding:20px 22px;position:relative}
[data-theme=dark] .sumcard.bad{background:radial-gradient(90% 160% at 0% 0%,rgba(239,68,68,.13),transparent 50%),linear-gradient(180deg,#151517,#111113)}
[data-theme=dark] .sumcard.wrn{background:radial-gradient(90% 160% at 0% 0%,rgba(234,179,8,.12),transparent 50%),linear-gradient(180deg,#151517,#111113)}
[data-theme=dark] .sumcard{background:radial-gradient(90% 160% at 0% 0%,rgba(163,230,53,.10),transparent 50%),linear-gradient(180deg,#151517,#111113)}
.score{filter:drop-shadow(0 0 16px color-mix(in srgb,var(--c) 35%,transparent))}
.kpis{border-radius:18px}
.kpi .sv{font-size:22px;font-weight:700;letter-spacing:-.02em}
.finding{border-radius:12px}
.cstats{border-radius:16px}
.chart{border-radius:16px}
.chrono .box{border-radius:14px}
.btn{border-radius:10px}
[data-theme=light] .news .nb{color:#3f6212}
[data-theme=light] .or,[data-theme=light] .hero .hint{color:#595959}
[data-theme=light] .pick .fmt i{color:#854d0e}
"#;
