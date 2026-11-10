fn main() -> Result<(), Box<dyn std::error::Error>> {
    let files = [
        "proto/spark/spark.proto",
        "proto/spark/spark_sampler.proto",
        "proto/spark/spark_heap.proto",
    ];
    for f in &files {
        println!("cargo:rerun-if-changed={f}");
    }
    let fds = protox::compile(files, ["proto"])?;
    prost_build::Config::new()
        .btree_map(["."])
        .compile_fds(fds)?;
    embed_browser_module()?;
    Ok(())
}

fn embed_browser_module() -> std::io::Result<()> {
    println!("cargo:rerun-if-env-changed=SA_WASM_PKG");
    let dir = std::env::var_os("SA_WASM_PKG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("web/pkg"));
    if dir.is_dir() {
        println!("cargo:rerun-if-changed={}", dir.display());
    }
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let glue = dir.join("sparkscope_web.js");
    let module = dir.join("sparkscope_web_bg.wasm");
    let complete = glue.is_file() && module.is_file();
    for (source, target) in [(glue, "sa_web.js"), (module, "sa_web.wasm")] {
        let bytes = if complete {
            println!("cargo:rerun-if-changed={}", source.display());
            std::fs::read(&source)?
        } else {
            Vec::new()
        };
        std::fs::write(out.join(target), bytes)?;
    }
    Ok(())
}
