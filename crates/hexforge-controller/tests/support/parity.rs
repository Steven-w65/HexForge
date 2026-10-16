//! Test-only evidence validation. Pending inventory is not a completion claim.
use serde_json::Value;
use std::collections::HashSet;

pub fn read_parity_fixture(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

pub fn validate_ledger(ledger: &Value, require_complete: bool) -> Vec<String> {
    let mut errors = Vec::new();
    let Some(entries) = ledger["entries"].as_array() else {
        return vec!["Ledger is empty".into()];
    };
    if entries.is_empty() {
        errors.push("Ledger is empty".into());
    }
    if ledger["declaredCases"].as_u64() != Some(entries.len() as u64) {
        errors.push("Declared case count differs from inventory".into());
    }
    let mut ids = HashSet::new();
    for entry in entries {
        let id = entry["id"].as_str().unwrap_or("");
        if !ids.insert(id) {
            errors.push(format!("Duplicate assertion id: {id}"));
        }
        if id.is_empty()
            || entry["source"]["path"].as_str().unwrap_or("").is_empty()
            || entry["source"]["name"].as_str().unwrap_or("").is_empty()
        {
            errors.push(format!("Missing source identity: {id}"));
        }
        let mapped = nonempty(&entry["native"]["test"]) && nonempty(&entry["native"]["file"]);
        if require_complete && (entry["status"] != "passed" || !mapped) {
            errors.push(format!("Unmapped or unverified: {id}"));
        }
        if entry["status"] == "passed"
            && (!nonempty(&entry["native"]["evidence"]["command"])
                || entry["native"]["evidence"]["result"] != "passed"
                || !nonempty(&entry["native"]["evidence"]["artifact"]))
        {
            errors.push(format!("Missing execution evidence: {id}"));
        }
    }
    errors
}

fn nonempty(value: &Value) -> bool {
    value.as_str().is_some_and(|text| !text.is_empty())
}

pub fn validate_capture(capture: &Value) -> Vec<String> {
    if capture["verified"] != true {
        return vec![];
    }
    let mut errors = Vec::new();
    if !capture["dpi"]
        .as_f64()
        .is_some_and(|dpi| dpi.is_finite() && dpi > 0.0)
    {
        errors.push("Verified capture needs recorded DPI".into());
    }
    if !nonempty(&capture["file"]) || !nonempty(&capture["theme"]) || !nonempty(&capture["state"]) {
        errors.push("Verified capture needs file, theme and state".into());
    }
    if !capture["clientSize"].as_array().is_some_and(|size| {
        size.len() == 2
            && size
                .iter()
                .all(|value| value.as_f64().is_some_and(|v| v.is_finite() && v > 0.0))
    }) {
        errors.push("Verified capture needs client size".into());
    }
    if capture["evidence"] != "lossless" {
        errors.push("Verified pixel comparison needs lossless evidence".into());
    }
    errors
}
