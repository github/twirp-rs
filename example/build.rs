use std::env;
use std::path::PathBuf;

use prost_wkt_build::*;

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").expect("failed to load OUT_DIR from environment"));
    let descriptor_file = out.join("descriptors.bin");
    let mut prost_build = prost_build::Config::new();

    let proto_source_files = protos();
    for entry in &proto_source_files {
        println!("cargo:rerun-if-changed={}", entry.display());
    }

    prost_build
        .service_generator(twirp_build::service_generator())
        .type_attribute(".", "#[derive(serde::Serialize,serde::Deserialize)]")
        .extern_path(".google.protobuf.Timestamp", "::prost_wkt_types::Timestamp")
        .file_descriptor_set_path(&descriptor_file)
        .compile_protos(&proto_source_files, &["./proto"])
        .expect("error compiling protos");

    let descriptor_bytes =
        fs_err::read(descriptor_file).expect("failed to read proto file descriptor");

    let descriptor = FileDescriptorSet::decode(&descriptor_bytes[..])
        .expect("failed to decode proto file descriptor");

    prost_wkt_build::add_serde(out, descriptor);
}

fn protos() -> Vec<PathBuf> {
    glob::glob("./proto/**/*.proto")
        .expect("io error finding proto files")
        .flatten()
        .collect()
}
