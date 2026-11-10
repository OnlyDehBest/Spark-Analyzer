#[path = "support/hprof_gen.rs"]
mod hprof_gen;

use sparkscope::hprof;

fn entry<'a>(h: &'a sparkscope::proto::HeapData, name: &str) -> &'a sparkscope::proto::HeapEntry {
    h.entries
        .iter()
        .find(|e| e.r#type == name)
        .unwrap_or_else(|| panic!("manca {name}"))
}

#[test]
fn histogram_from_heap_dump() {
    let h = hprof::parse_bytes(&hprof_gen::sample(10)).unwrap();
    let s = entry(&h, "java.lang.String");
    assert_eq!(s.instances, 500);
    // 16 bytes of fields with 1 reference: 12 header + 16 - 4 = 24
    assert_eq!(s.size, 500 * 24);
    let d = entry(&h, "com.example.crates.data.PlayerCrateData");
    assert_eq!(d.instances, 400);
    assert_eq!(d.size, 400 * 32);
    let b = entry(&h, "byte[]");
    assert_eq!((b.instances, b.size), (300, 300 * 120));
    let a = entry(&h, "java.lang.Object[]");
    assert_eq!((a.instances, a.size), (20, 20 * 56));
    assert!(h.metadata.unwrap().generated_time > 0);
    assert!(h.entries.windows(2).all(|w| w[0].size >= w[1].size));
}

#[test]
fn chunk_size_and_gzip_do_not_change_the_result() {
    let dump = hprof_gen::sample(7);
    let whole = hprof::parse_bytes(&dump).unwrap();
    for size in [1usize, 3, 17, 4096] {
        let mut s = hprof::HprofStream::new();
        for c in dump.chunks(size) {
            s.feed(c).unwrap();
        }
        assert_eq!(s.finish().unwrap(), whole, "chunk {size}");
    }
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    std::io::Write::write_all(&mut gz, &dump).unwrap();
    let gz = gz.finish().unwrap();
    let mut s = hprof::HprofStream::new();
    for c in gz.chunks(1000) {
        s.feed(c).unwrap();
    }
    assert_eq!(s.finish().unwrap(), whole);
}

#[test]
fn truncated_or_wrong_files_are_rejected() {
    let dump = hprof_gen::sample(3);
    assert!(hprof::parse_bytes(&dump[..dump.len() - 50]).is_err());
    assert!(hprof::parse_bytes(b"not a dump at all").is_err());
}

#[test]
fn class_names_are_java_style() {
    assert_eq!(hprof::java_name("java/lang/String"), "java.lang.String");
    assert_eq!(
        hprof::java_name("[Ljava/lang/Object;"),
        "java.lang.Object[]"
    );
    assert_eq!(hprof::java_name("[[I"), "int[][]");
    assert_eq!(hprof::java_name("[B"), "byte[]");
}

#[test]
fn heap_dump_goes_through_the_heap_report_and_comparison() {
    let before = hprof_gen::sample(5);
    let after = hprof_gen::sample(50);
    let loaded = sparkscope::loader::load_bytes(&after, "server.hprof").unwrap();
    assert!(matches!(
        loaded.payload,
        sparkscope::loader::Payload::Heap(_)
    ));
    let c =
        sparkscope::heapdiff::compare_bytes(&before, "prima.hprof", &after, "dopo.hprof").unwrap();
    assert!(c.after.objects > c.before.objects);
}

#[test]
fn plugins_are_found_from_their_class_loaders() {
    let mut g = hprof_gen::Gen::new();
    let object = g.class("java/lang/Object", 0, 0, 0);
    let java_plugin = g.class("org/bukkit/plugin/java/JavaPlugin", object, 0, 0);
    let pcl = g.class("org/bukkit/plugin/java/PluginClassLoader", object, 0, 0);
    let loader = g.new_id();
    let main = g.class_in("me/example/shopplus/ShopPlus", java_plugin, loader, 1, 0);
    let price = g.class_in("me/example/shopplus/data/PriceEntry", object, loader, 1, 2);
    let shaded = g.class_in("com/zaxxer/hikari/PoolEntry", object, loader, 1, 1);
    g.instance_with_id(loader, pcl, 8);
    g.instances(main, 1, 8);
    g.instances(price, 1000, 16);
    g.instances(shaded, 500, 12);
    let h = hprof::parse_bytes(&g.finish()).unwrap();
    let meta = h.metadata.clone().unwrap();
    assert!(
        meta.sources.contains_key("ShopPlus"),
        "{:?}",
        meta.sources.keys()
    );
    let r = sparkscope::heap::analyze(&h);
    let shop = r
        .by_plugin
        .iter()
        .find(|p| p.name == "ShopPlus")
        .expect("plugin");
    assert_eq!(
        shop.instances, 1501,
        "shaded classes belong to the plugin too"
    );
}

#[test]
fn summary_read_in_the_browser_gives_the_same_report_as_the_dump() {
    let dump = hprof_gen::sample(10);
    let summary = prost::Message::encode_to_vec(&hprof::parse_bytes(&dump).unwrap());
    assert!(summary.len() < dump.len());
    let render = |bytes: &[u8]| {
        let loaded = sparkscope::loader::load_bytes(bytes, "server.hprof").unwrap();
        let (r, _, heap) = sparkscope::pipeline::build(loaded, None).unwrap();
        sparkscope::webui::render_report(&r, heap.as_ref()).html
    };
    let from_dump = render(&dump);
    assert_eq!(render(&summary), from_dump);
    assert!(from_dump.contains("heap dump (.hprof)"));
}
