extern crate tonic_build;

fn main() {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile(&["proto/cluster.proto"], &["proto/"])
        .unwrap_or_else(|e| panic!("Compilation failed :( {}", e));
}
