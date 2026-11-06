use anyhow::{Result, bail};

use crate::analysis;
use crate::heap;
use crate::loader::{Loaded, Payload};
use crate::model::Profile;
use crate::report::{self, Report};

pub type Built = (Report, Option<Profile>, Option<heap::HeapReport>);

pub fn build(loaded: Loaded, thread: Option<&str>) -> Result<Built> {
    build_with(loaded, thread, None)
}

pub fn build_with(
    loaded: Loaded,
    thread: Option<&str>,
    windows: Option<&[usize]>,
) -> Result<Built> {
    let kind = loaded.payload.kind();
    match loaded.payload {
        Payload::Sampler(s) => {
            let mut p = Profile::from_proto(*s);
            if let Some(t) = thread
                && !p.select_thread(t)
            {
                let names: Vec<&str> = p.threads.iter().map(|t| t.name.as_str()).collect();
                bail!(
                    "{}",
                    tf!(
                        "thread '{t}' non trovato. Disponibili: {}",
                        "thread '{t}' not found. Available: {}",
                        names.join(", ")
                    )
                );
            }
            if let Some(w) = windows {
                p.restrict_windows(w);
            }
            let a = analysis::analyze(&p);
            let r = report::build(&p, a, &loaded.origin, loaded.viewer_url, kind);
            Ok((r, Some(p), None))
        }
        Payload::Health(h) => {
            let p = Profile::from_health(*h);
            let a = analysis::analyze(&p);
            let r = report::build(&p, a, &loaded.origin, loaded.viewer_url, kind);
            Ok((r, None, None))
        }
        Payload::Heap(h) => {
            let hm = h.metadata.clone().unwrap_or_default();
            let hr = heap::analyze(&h);
            let health = crate::proto::HealthData {
                metadata: Some(crate::proto::HealthMetadata {
                    creator: hm.creator,
                    platform_metadata: hm.platform_metadata,
                    platform_statistics: hm.platform_statistics,
                    system_statistics: hm.system_statistics,
                    generated_time: hm.generated_time,
                    server_configurations: hm.server_configurations,
                    sources: hm.sources,
                    extra_platform_metadata: hm.extra_platform_metadata,
                    metrics: hm.metrics,
                }),
                ..Default::default()
            };
            let p = Profile::from_health(health);
            let a = analysis::analyze(&p);
            let r = report::build(&p, a, &loaded.origin, loaded.viewer_url, kind);
            Ok((r, None, Some(hr)))
        }
    }
}
