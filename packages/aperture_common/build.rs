fn main() {
    if !std::path::Path::new("src/msg_instantiate_contract_response.rs").exists() {
        protoc_rust::Codegen::new()
            .out_dir("src")
            .inputs(&["src/msg_instantiate_contract_response.proto"])
            .run()
            .expect("Running protoc failed.");
    }
}
