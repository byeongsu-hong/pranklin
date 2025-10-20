fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_files = vec!["./proto/evnode/v1/execution.proto"];

    tonic_prost_build::configure().compile_protos(&proto_files, &["./proto"])?;

    println!("cargo:rerun-if-changed=build.rs");

    for file in &proto_files {
        println!("cargo:rerun-if-changed={}", file);
    }

    Ok(())
}
