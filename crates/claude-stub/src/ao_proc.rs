fn main() {
    println!("{}", serde_json::Value::Array(orchestrator_core::processes::list()));
}
