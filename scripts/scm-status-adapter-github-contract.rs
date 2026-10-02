// Keep digest, freshness, identity and monotonic rules in erf-status, not Python.
use erf_status::{FakeStatusAdapter, StatusAdapterDeployment, StatusPublication, StatusPublicationAdapter};
use serde_json::{json, Value};
use std::io;

fn check(value: Value) -> Result<Value, String> {
    let publication: StatusPublication = serde_json::from_value(value["publication"].clone())
        .map_err(|_| "invalid publication shape")?;
    let deployment: StatusAdapterDeployment = serde_json::from_value(value["deployment"].clone())
        .map_err(|_| "invalid deployment shape")?;
    let now = value["now"].as_u64().ok_or("invalid clock")?;
    deployment.validate_for(&publication, now).map_err(|e| e.to_string())?;
    let mut adapter = FakeStatusAdapter::default();
    if !value["previous"].is_null() {
        let previous: StatusPublication = serde_json::from_value(value["previous"].clone())
            .map_err(|_| "invalid ledger publication")?;
        // Historical accepted evidence can now be expired; new input cannot.
        adapter.accept(&previous, previous.issued_at_epoch_secs).map_err(|e| e.to_string())?;
    }
    let receipt = adapter.deploy(&deployment, &publication, now).map_err(|e| e.to_string())?;
    Ok(receipt.as_json())
}

fn main() {
    let result = serde_json::from_reader(io::stdin()).map_err(|_| "invalid JSON".to_string()).and_then(check);
    match result {
        Ok(receipt) => println!("{}", json!({"receipt": receipt})),
        Err(reason) => { println!("{}", json!({"reason": reason})); std::process::exit(2); }
    }
}
