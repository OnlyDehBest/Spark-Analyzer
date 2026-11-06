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
    Ok(())
}
