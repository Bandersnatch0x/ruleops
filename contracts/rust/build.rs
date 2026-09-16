use std::env;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let proto_root = manifest.join("../../contracts/proto");
    println!("cargo:rerun-if-changed=../../contracts/proto/backend_host.proto");

    // PROTOC must be provided by the environment (see scripts/check.sh and
    // README); PATH fallback is "protoc".
    if env::var("PROTOC").is_err() {
        env::set_var("PROTOC", "protoc");
    }

    prost_build::Config::new()
        .compile_protos(&["backend_host.proto"], &[proto_root])
        .expect("compile backend_host.proto: is PROTOC or protoc on PATH available?");
}
