use serde_json::json;
#[path = "support/parity.rs"]
mod parity;

#[test]
fn duplicate_assertion_ids_are_rejected() {
    let entry = json!({"id":"one","status":"pending","source":{"path":"test.ts","name":"case"},"native":null});
    let ledger = json!({"declaredCases":2,"entries":[entry.clone(),entry]});
    assert!(parity::validate_ledger(&ledger, false)
        .iter()
        .any(|error| error.contains("Duplicate assertion id")));
}

#[test]
fn missing_native_mapping_is_rejected() {
    let mut ledger = parity::read_parity_fixture("ledger.json");
    assert_eq!(ledger["declaredCases"], 606);
    assert!(parity::validate_ledger(&ledger, false).is_empty());
    // Completion of the real inventory must not break the validator's negative
    // test. Remove a controlled mapping instead of relying on permanent gaps.
    ledger["entries"][0]["status"] = json!("pending");
    ledger["entries"][0]["native"] = json!(null);
    assert!(parity::validate_ledger(&ledger, true)
        .iter()
        .any(|error| error.contains("Unmapped")));
}

#[test]
fn passed_status_requires_execution_evidence() {
    let mut entry = json!({"id":"one","status":"passed","source":{"path":"test.ts","name":"case"},"native":{"file":"test.rs","test":"case"}});
    assert!(
        parity::validate_ledger(&json!({"declaredCases":1,"entries":[entry.clone()]}), true)
            .iter()
            .any(|error| error.contains("execution evidence"))
    );
    entry["native"]["evidence"] =
        json!({"command":"cargo test","result":"passed","artifact":"tests.log"});
    assert!(
        parity::validate_ledger(&json!({"declaredCases":1,"entries":[entry]}), true).is_empty()
    );
}

#[test]
fn visual_capture_requires_dpi_and_state() {
    let capture = json!({"file":"main.png","theme":"dark","state":"empty","clientSize":[1280,800],"dpi":96,"evidence":"lossless","verified":true});
    assert!(parity::validate_capture(&capture).is_empty());
    let mut invalid = capture;
    invalid["dpi"] = json!(null);
    assert!(!parity::validate_capture(&invalid).is_empty());
}
