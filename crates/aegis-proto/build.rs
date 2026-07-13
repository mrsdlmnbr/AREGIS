// Generates Rust wire types from the source of truth (spec §8). Requires
// `protoc` on PATH (CI installs protobuf-compiler).
fn main() {
    println!("cargo:rerun-if-changed=../../proto/aegis/v1/aegis.proto");
    prost_build::Config::new()
        .compile_protos(&["../../proto/aegis/v1/aegis.proto"], &["../../proto"])
        .expect("protoc failed — install protobuf-compiler");
}
