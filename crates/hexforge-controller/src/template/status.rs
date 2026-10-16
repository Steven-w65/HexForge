#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateSource {
    None,
    Draft,
    File,
}
/// Persistence and parse freshness are independent; Save never implies Apply.
pub fn template_status(
    source: TemplateSource,
    dirty: bool,
    applied: bool,
    needs_refresh: bool,
    has_file: bool,
    has_diagnostics: bool,
) -> String {
    if source == TemplateSource::None {
        return "No template".into();
    }
    let persistence = match source {
        TemplateSource::Draft => "Unsaved draft",
        _ if dirty => "Modified",
        _ => "Saved",
    };
    let parse = if !has_file {
        if source == TemplateSource::File {
            "Loaded"
        } else {
            ""
        }
    } else if applied {
        if has_diagnostics {
            "Parse errors"
        } else {
            "Applied"
        }
    } else if needs_refresh {
        "Needs apply"
    } else {
        "Pending apply"
    };
    if parse.is_empty() {
        persistence.into()
    } else {
        format!("{persistence} · {parse}")
    }
}
