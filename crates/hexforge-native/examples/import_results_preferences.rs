//! Development-only, one-shot import of the two retained frontend preferences.
//! This is not linked into the portable application and never reads a WebView.

use hexforge_native::platform::preferences::{
    PreferenceStore, ResultsPreferences, WindowsPreferenceStore,
};
use std::io::Read;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferencePreferences {
    right_collapsed: Option<String>,
    results_pane_height: Option<String>,
}
fn read_reference_preferences(input: impl Read) -> Result<ResultsPreferences, String> {
    let mut bytes = Vec::new();
    input
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 4096 {
        return Err("Reference preference export exceeds 4096 bytes.".into());
    }
    let values: ReferencePreferences = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok(ResultsPreferences {
        height: values
            .results_pane_height
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(220.0),
        collapsed: values.right_collapsed.as_deref() == Some("true"),
    }
    .normalized())
}
fn main() -> std::process::ExitCode {
    if !cfg!(debug_assertions) {
        eprintln!("Reference preference import is a development-only tool.");
        return std::process::ExitCode::FAILURE;
    }
    let result = read_reference_preferences(std::io::stdin().lock()).and_then(|values| {
        WindowsPreferenceStore
            .write_results_preferences(&values)
            .map_err(|e| e.to_string())?;
        println!(
            "Imported results pane preferences: height={}, collapsed={}",
            values.height, values.collapsed
        );
        Ok(())
    });
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Preference import aborted: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_preserves_the_reference_values_and_defaults() {
        assert_eq!(
            read_reference_preferences(
                br#"{"rightCollapsed":"true","resultsPaneHeight":"345.5"}"#.as_slice()
            )
            .unwrap(),
            ResultsPreferences {
                height: 345.5,
                collapsed: true
            }
        );
        assert_eq!(
            read_reference_preferences(
                br#"{"rightCollapsed":null,"resultsPaneHeight":null}"#.as_slice()
            )
            .unwrap(),
            ResultsPreferences::default()
        );
        for value in ["NaN", "0", "27", "", "Infinity"] {
            let json = format!(r#"{{"rightCollapsed":"false","resultsPaneHeight":"{value}"}}"#);
            assert_eq!(
                read_reference_preferences(json.as_bytes()).unwrap(),
                ResultsPreferences::default()
            );
        }
    }
    #[test]
    fn import_rejects_unrelated_or_oversized_data_before_writing() {
        assert!(read_reference_preferences(br#"{"filePath":"private.bin"}"#.as_slice()).is_err());
        assert!(read_reference_preferences(br#"{"rightCollapsed":true}"#.as_slice()).is_err());
        assert!(read_reference_preferences(vec![b' '; 4097].as_slice()).is_err());
    }
}
