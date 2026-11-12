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
.news{box-shadow:0 12px 30px -18px rgba(163,230,53,.45)}
[data-theme=dark] .news{background:rgba(19,19,20,.85);border-color:rgba(163,230,53,.22)}
.news .nb{box-shadow:0 0 14px rgba(163,230,53,.35)}
.hcard{position:relative;border-radius:22px;border-color:transparent}
[data-theme=dark] .hcard{background:radial-gradient(120% 150% at 0% 0%,rgba(163,230,53,.13),transparent 48%),radial-gradient(90% 130% at 100% 100%,rgba(56,189,248,.15),transparent 55%),linear-gradient(180deg,#161618,#111113)!important;box-shadow:0 40px 90px -50px rgba(0,0,0,.95)}
.hcard::after{content:"";position:absolute;inset:0;border-radius:inherit;padding:1px;pointer-events:none;background:linear-gradient(135deg,rgba(163,230,53,.55),rgba(255,255,255,.06) 38%,rgba(255,255,255,.04) 62%,rgba(56,189,248,.55));-webkit-mask:linear-gradient(#000 0 0) content-box,linear-gradient(#000 0 0);-webkit-mask-composite:xor;mask-composite:exclude}
.hcard .hlogo{filter:drop-shadow(0 12px 30px rgba(163,230,53,.28))}
.hcard h1{font-weight:800;letter-spacing:-.035em;line-height:1.05}
[data-theme=dark] .hcard h1{background:linear-gradient(180deg,#ffffff 30%,#9a9a9a);-webkit-background-clip:text;background-clip:text;color:transparent}
.hcard .wave polyline{stroke-width:2.6;filter:drop-shadow(0 0 7px rgba(163,230,53,.55))}
.htag{line-height:1.6;max-width:700px}
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
@media(min-width:701px){.news{border-radius:999px}.hcard{padding:42px 40px 34px}.hcard .hlogo{width:68px;height:68px}.hcard h1{font-size:56px;letter-spacing:-.04em;line-height:1}.hcard .wave{opacity:.9}.htag{font-size:17px;margin-top:18px}}
@media(max-width:700px){#lform .btn,.slinkform .btn{width:100%;justify-content:center}.news{border-radius:14px}.hcard{padding:24px 20px 76px}.hcard .wave{opacity:.45;bottom:10px}.hcard h1{font-size:34px}.hcard .hlogo{width:48px;height:48px}.htag{font-size:15.5px}.chips span{font-size:12px;padding:5px 11px}}
[data-theme=light] .news .nb{color:#3f6212}
[data-theme=light] .or,[data-theme=light] .hero .hint{color:#595959}
[data-theme=light] .pick .fmt i{color:#854d0e}
.usage{position:relative;display:flex;flex-wrap:wrap;gap:12px 34px;margin-top:22px;padding-top:20px;border-top:1px solid rgba(255,255,255,.07)}
[data-theme=light] .usage{border-top-color:var(--line)}
.ust{display:flex;align-items:center;gap:12px}
.usi{width:40px;height:40px;border-radius:12px;display:grid;place-items:center;flex:none;color:var(--ok);background:rgba(163,230,53,.10);border:1px solid rgba(163,230,53,.22)}
.ust+.ust .usi{color:var(--acc);background:rgba(56,189,248,.10);border-color:rgba(56,189,248,.24)}
.usi svg{width:20px;height:20px}
.ust b{display:block;font-size:24px;line-height:1.1;font-weight:800;letter-spacing:-.02em;color:var(--hd);font-variant-numeric:tabular-nums}
.ust span:not(.usi){display:block;font-size:12.5px;color:var(--muted);margin-top:2px}
@media(max-width:700px){.usage{gap:14px 24px;margin-top:18px;padding-top:16px}.ust b{font-size:20px}.usi{width:36px;height:36px}}
html{-webkit-text-size-adjust:100%;text-size-adjust:100%}
a,button,label,summary,.chip,.ghost{-webkit-tap-highlight-color:transparent}
@media(min-width:701px) and (max-width:1100px){.hcard{padding-bottom:86px}.hcard .wave{width:100%;right:0;top:auto;bottom:10px;height:60px;opacity:.55}.kpis{grid-template-columns:repeat(4,minmax(0,1fr))}}
@media(max-height:520px) and (orientation:landscape){.hcard{padding:18px 22px 20px}.hcard .wave{display:none}.hcard h1{font-size:30px}.hcard .hlogo{width:40px;height:40px}.htag{font-size:14.5px;margin-top:8px}.chips{margin-top:10px}.news{margin-bottom:10px}.hero{padding:20px 20px 18px}.hero .dicon{display:none}.hero h2{margin-top:0}}
@media(pointer:coarse){.ghost{width:44px;height:44px}.chip{min-height:36px}}
@media(pointer:coarse) and (max-width:900px){.side nav a{min-width:44px;min-height:44px;justify-content:center}}
@media(max-width:900px){.side{padding-top:max(8px,env(safe-area-inset-top));padding-left:max(12px,env(safe-area-inset-left));padding-right:max(12px,env(safe-area-inset-right))}.content{padding-left:max(16px,env(safe-area-inset-left));padding-right:max(16px,env(safe-area-inset-right));padding-bottom:env(safe-area-inset-bottom)}}
::selection{background:rgba(56,189,248,.32)}
*{scrollbar-width:thin;scrollbar-color:var(--card3) transparent}
.hcard::before{content:"";position:absolute;inset:0;pointer-events:none;background-image:radial-gradient(rgba(255,255,255,.13) 1px,transparent 1.5px);background-size:22px 22px;-webkit-mask-image:radial-gradient(70% 120% at 88% 0%,#000,transparent 72%);mask-image:radial-gradient(70% 120% at 88% 0%,#000,transparent 72%)}
[data-theme=light] .hcard::before{background-image:radial-gradient(rgba(10,10,10,.13) 1px,transparent 1.5px)}
.hcard>*{position:relative}
.hcard>.wave{position:absolute}
.hcard .wave polyline{stroke-dasharray:2400;animation:sa-wave 1.8s cubic-bezier(.3,.7,.2,1) .15s both}
@keyframes sa-wave{from{stroke-dashoffset:2400}to{stroke-dashoffset:0}}
.hero .dicon{position:relative}
.hero .dicon::before,.hero .dicon::after{content:"";position:absolute;inset:-13px;border-radius:28px;border:1px solid rgba(56,189,248,.26);pointer-events:none;transition:inset .25s ease,border-color .25s ease}
.hero .dicon::after{inset:-28px;border-radius:42px;border-color:rgba(56,189,248,.12)}
.hero:hover .dicon::before,.hero.over .dicon::before{inset:-16px;border-color:rgba(56,189,248,.45)}
.hero:hover .dicon::after,.hero.over .dicon::after{inset:-34px;border-color:rgba(56,189,248,.2)}
.hero h2{margin-top:36px}
.hero .hint{text-wrap:balance;max-width:560px;margin-left:auto;margin-right:auto;line-height:1.9}
.recent a{border-radius:9px;padding:8px 10px;transition:background-color .15s ease}
.recent i{box-shadow:0 0 0 3px color-mix(in srgb,currentColor 0%,transparent)}
[data-theme=light] body{background:#f6f7f9}
[data-theme=light] .content{position:relative;isolation:isolate}
[data-theme=light] .content::before{content:"";position:fixed;inset:0;z-index:-1;pointer-events:none;background:radial-gradient(900px 520px at 78% -12%,rgba(2,132,199,.09),transparent 62%),radial-gradient(760px 460px at 22% -10%,rgba(101,163,13,.08),transparent 62%)}
[data-theme=light] .side{background:#fff}
[data-theme=light] .hcard{box-shadow:0 30px 70px -50px rgba(15,23,42,.35)}
[data-theme=light] .hero{background:#fff;border-color:var(--line);box-shadow:0 30px 70px -50px rgba(15,23,42,.3)}
[data-theme=light] .lrow input{background:#f7f8fa}
[data-theme=light] .panel,[data-theme=light] .sumcard,[data-theme=light] .kpis{box-shadow:0 24px 60px -48px rgba(15,23,42,.3)}
.top h1{font-weight:800;letter-spacing:-.03em}
[data-theme=dark] .top h1{background:linear-gradient(180deg,#fff 35%,#a3a3a3);-webkit-background-clip:text;background-clip:text;color:transparent}
.panel>h2{gap:11px;margin-bottom:14px}
.panel>h2>.hi{width:30px;height:30px;border-radius:9px;display:grid;place-items:center;flex:none;color:var(--acc);background:rgba(56,189,248,.10);border:1px solid rgba(56,189,248,.22)}
.bar{height:7px}
[data-theme=dark] .bar{background:rgba(255,255,255,.07)}
.bar i{background:linear-gradient(90deg,var(--bar),color-mix(in srgb,var(--bar) 55%,#fff));box-shadow:0 0 10px -2px var(--bar)}
[data-theme=dark] thead th{background:rgba(255,255,255,.025)}
[data-theme=light] thead th{background:#fafafa}
[data-theme=dark] tbody tr:hover{background:rgba(255,255,255,.035)}
.panel>table+table,.panel>div>table+table{margin-top:14px}
.finding{transition:border-color .2s ease,background-color .2s ease,box-shadow .2s ease}
[data-theme=dark] .finding:hover{background:#1c1c1f;box-shadow:0 14px 30px -22px rgba(0,0,0,.9)}
[data-theme=dark] .finding.crit{background:linear-gradient(90deg,rgba(239,68,68,.07),transparent 38%),var(--card2)}
.sev{letter-spacing:.06em}
@media(prefers-reduced-motion:reduce){.hcard .wave polyline{animation:none}}
[data-theme] .content::before{display:none}
[data-theme=light] body{background:#fafafa}
.side{border-radius:10px;box-shadow:none}
[data-theme=dark] .side{background:var(--side)}
.side nav a{border-radius:6px}
.side nav a.on{background:var(--card3);box-shadow:inset 2px 0 0 var(--acc)}
.sidefoot{border-radius:8px}
.news,.news .nb{box-shadow:none}
@media(min-width:701px){.news{border-radius:8px}}
.hcard{border-radius:10px;border-color:var(--line);box-shadow:none!important}
[data-theme=dark] .hcard{background:var(--card)!important}
[data-theme=light] .hcard{background:#fff!important}
.hcard::after{display:none}
.hcard::before{background-image:linear-gradient(var(--line) 1px,transparent 1px),linear-gradient(90deg,var(--line) 1px,transparent 1px)!important;background-size:44px 22px;-webkit-mask-image:linear-gradient(90deg,transparent 52%,#000 78%);mask-image:linear-gradient(90deg,transparent 52%,#000 78%);opacity:.55}
.hcard .hlogo{filter:none}
.hcard h1{font-weight:700;letter-spacing:-.025em}
[data-theme=dark] .hcard h1{background:none;color:var(--hd)}
.hcard .wave polyline{stroke-width:1.6;filter:none}
[data-theme=dark] .htag{color:#a8a8a8}
.chips{gap:8px}
.chips span{border-radius:6px;padding:5px 10px;font-size:12px}
[data-theme=dark] .chips span{background:var(--card2);border-color:var(--line);color:#bdbdbd}
.usage{gap:0;margin-top:20px;padding-top:18px}
.usi{display:none}
.ust{padding-right:28px;margin-right:28px;border-right:1px solid var(--line)}
.ust:last-child{border-right:0;margin-right:0}
.ust b{font-family:"JetBrains Mono",ui-monospace,monospace;font-size:20px;font-weight:600;letter-spacing:-.02em}
.ust span:not(.usi){font-size:12px}
.seg.big{border-radius:8px;padding:3px}
.seg.big button{border-radius:6px}
[data-theme=dark] .seg.big{background:var(--card)}
[data-theme=dark] .seg.big button.on{background:var(--card3);box-shadow:none}
.seg.big button.on{box-shadow:none}
.hero{border-radius:10px;padding:32px 28px 26px;box-shadow:none!important}
[data-theme=dark] .hero{background:var(--card);border-color:var(--line)}
.hero:hover,.hero.over{border-color:var(--acc)}
.hero .dicon{width:40px;height:40px;border-radius:8px;background:var(--card2);border:1px solid var(--line);color:var(--acc);box-shadow:none}
.hero .dicon svg{width:18px;height:18px}
.hero .dicon::before,.hero .dicon::after{display:none}
.hero h2{font-size:17px;font-weight:600;margin-top:14px;letter-spacing:-.01em}
.lrow input{height:42px;border-radius:8px;padding:0 14px;font-size:14px;font-family:"JetBrains Mono",ui-monospace,monospace}
.lrow input:focus{box-shadow:0 0 0 3px rgba(56,189,248,.14)}
#lform .btn,.slinkform .btn{height:42px;border-radius:8px;padding:0 18px;box-shadow:none;font-weight:600}
.pick{min-height:42px;border-radius:8px}
.pick .fmt i{box-shadow:none}
.or{margin:16px auto}
.busy{border-radius:10px}
.panel{border-radius:10px;box-shadow:none!important}
[data-theme=dark] .panel{background:var(--card)}
.sumcard{border-radius:10px;box-shadow:none!important}
[data-theme=dark] .sumcard,[data-theme=dark] .sumcard.bad,[data-theme=dark] .sumcard.wrn{background:var(--card)}
.score{filter:none}
.kpis{border-radius:10px;box-shadow:none!important}
.kpi .sv{font-family:"JetBrains Mono",ui-monospace,monospace;font-size:20px;font-weight:600;letter-spacing:-.03em}
.top h1{font-size:26px;font-weight:700;letter-spacing:-.02em}
[data-theme=dark] .top h1{background:none;color:var(--hd)}
.sub{font-size:13.5px}
.panel>h2>.hi{width:auto;height:auto;border:0;background:none;color:var(--muted)}
.panel>h2{gap:8px;font-size:15px}
.bar{height:5px;border-radius:2px}
.bar i{background:var(--bar);box-shadow:none;border-radius:2px}
.panel>table,.grid2 table,.panel>div>table{border-radius:8px}
td{padding:9px 14px}
th{padding:9px 14px;font-family:"JetBrains Mono",ui-monospace,monospace;font-size:11px}
.finding{border-radius:8px}
[data-theme=dark] .finding.crit{background:var(--card2)}
[data-theme=dark] .finding:hover{background:var(--card3);box-shadow:none}
.cstats,.chart,.spike,#fwrap{border-radius:8px}
.chrono .box{border-radius:8px}
.btn{border-radius:8px}
@media(min-width:701px){.hcard{padding:30px 32px 26px}.hcard .hlogo{width:44px;height:44px}.hcard h1{font-size:34px;letter-spacing:-.025em}.hcard .wave{opacity:.75}.htag{font-size:15px;margin-top:14px;max-width:620px}}
@media(max-width:700px){.news{border-radius:8px}.hcard h1{font-size:28px}.ust{padding-right:18px;margin-right:18px}.ust b{font-size:18px}.hero{padding:22px 16px 18px}}
.hprev{display:none}
@media(min-width:1280px){.hcard{display:grid;grid-template-columns:minmax(0,1fr) 448px;gap:0 44px;align-items:center;padding:34px 34px 34px 36px}.hcard>.wave{display:none}.hcard::before{-webkit-mask-image:linear-gradient(90deg,transparent 30%,#000 70%);mask-image:linear-gradient(90deg,transparent 30%,#000 70%)}.hprev{display:block;background:var(--bg);border:1px solid var(--line);border-radius:10px;padding:14px 16px 12px;font-size:12px;box-shadow:0 30px 60px -34px rgba(0,0,0,.85)}[data-theme=light] .hprev{background:#fff;box-shadow:0 30px 60px -40px rgba(15,23,42,.35)}}
.pvh{display:flex;align-items:center;gap:8px;padding-bottom:11px;margin-bottom:12px;border-bottom:1px solid var(--line);color:var(--muted);font-size:11.5px}
.pvh i{width:7px;height:7px;border-radius:50%;background:var(--crit);box-shadow:0 0 0 3px color-mix(in srgb,var(--crit) 22%,transparent)}
.pvh span{color:var(--hd);font-weight:600}
.pvh em{font-style:normal;font-family:"JetBrains Mono",ui-monospace,monospace;font-size:11px}
.pvk{display:grid;grid-template-columns:52px repeat(3,1fr);gap:0;align-items:center;margin-bottom:10px}
.pvk>div+div{padding-left:14px;border-left:1px solid var(--line)}
.pvk>div:nth-child(2){margin-left:14px}
.pvk small{display:block;color:var(--muted);font-size:11px;margin-bottom:2px}
.pvk b{font-family:"JetBrains Mono",ui-monospace,monospace;font-size:19px;font-weight:600;color:var(--hd);letter-spacing:-.03em}
.pvk b.c{color:var(--crit)}.pvk b.h{color:var(--high)}
.pvk u{text-decoration:none;font-size:11px;color:var(--muted);margin-left:4px;font-weight:400}
.pvr{position:relative;width:52px;height:52px}
.pvr svg{width:52px;height:52px;transform:rotate(-90deg)}
.pvr circle{fill:none;stroke:var(--card3);stroke-width:4}
.pvr circle.v{stroke:var(--crit);stroke-linecap:round}
.pvr b{position:absolute;inset:0;display:grid;place-items:center;font-size:15px}
.pvc{display:block;width:100%;height:104px;margin:0 0 10px;border:1px solid var(--line);border-radius:8px;background:var(--card)}
.pvc path{fill:none;vector-effect:non-scaling-stroke}
.pvc .g{stroke:var(--line2);stroke-width:1}
.pvc .lim{stroke:var(--crit);stroke-width:1;stroke-dasharray:4 4;opacity:.6}
.pvc .ar{fill:url(#pvg)}
.pvc .l1{stroke:var(--high);stroke-width:1.8}
.pvc .l2{stroke:var(--acc);stroke-width:1.4}
.pvf{display:flex;align-items:center;gap:10px;padding:7px 10px;margin-top:6px;border:1px solid var(--line);border-left:2px solid var(--crit);border-radius:6px;background:var(--card2)}
.pvf.h{border-left-color:var(--high)}
.pvf span{flex:none;font-family:"JetBrains Mono",ui-monospace,monospace;font-size:9.5px;font-weight:700;letter-spacing:.05em;padding:2px 6px;border-radius:4px;color:var(--crit);background:color-mix(in srgb,var(--crit) 14%,transparent)}
.pvf.h span{color:var(--high);background:color-mix(in srgb,var(--high) 14%,transparent)}
.pvf p{margin:0;flex:1;min-width:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;color:var(--hd);font-weight:500}
.pvf b{font-family:"JetBrains Mono",ui-monospace,monospace;font-weight:600;color:var(--hd)}
@media(min-width:1280px){.hcard h1{font-size:42px;letter-spacing:-.03em}.hcard .hlogo{width:54px;height:54px}.htag{font-size:16px;margin-top:18px}.chips{margin-top:22px}.usage{margin-top:26px;padding-top:22px}}
.hero .dicon{color:var(--acc)!important}
.pvh{background:var(--card2);margin:-14px -16px 12px;padding:9px 10px 9px 14px;border-radius:9px 9px 0 0}
.pvh em{padding-left:8px;border-left:1px solid var(--line)}
.pvw{margin-left:auto;display:flex;gap:6px}
.pvw svg{width:20px;height:20px;padding:5px;border-radius:50%;background:var(--card3);fill:none;stroke:var(--fg);stroke-width:1.2;stroke-linecap:round;stroke-linejoin:round}
"#;
