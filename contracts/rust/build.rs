use std::env;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let proto_root = manifest.join("../../contracts/proto");
    let protoc = env::var("PROTOC").unwrap_or_else(|_| {
        // Documented fallback for this project's user-local toolchain layout;
        // CI and other hosts should export PROTOC explicitly.
        env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".local/opt/protoc/bin/protoc"))
            .filter(|p| p.exists())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "protoc".to_string())
    });
    env::set_var("PROTOC", protoc);

    println!(
        "cargo:rerun-if-changed=../../contracts/proto/backend_host.proto"
    );
    prost_build::Config::new()
        .compile_protos(&["backend_host.proto"], &[proto_root])
        .expect("compile backend_host.proto");
}
