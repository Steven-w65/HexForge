use crate::session::PageData;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MinimapSampleRowDto {
    pub row: String,
    pub bytes: Vec<u8>,
    pub modified_offsets: Vec<String>,
}

impl MinimapSampleRowDto {
    pub fn from_page(row: u64, page: PageData) -> Self {
        Self {
            row: row.to_string(),
            bytes: page.bytes,
            modified_offsets: page
                .modified_offsets
                .into_iter()
                .map(|value| value.to_string())
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MinimapSamplesResponse {
    pub revision: String,
    pub samples: Vec<MinimapSampleRowDto>,
}

#[cfg(test)]
mod tests {
    use super::{MinimapSampleRowDto, MinimapSamplesResponse};
    use crate::session::PageData;

    #[test]
    fn minimap_dto_serializes_large_positions_as_decimal_strings() {
        let row = MinimapSampleRowDto::from_page(
            9007199254740993,
            PageData {
                offset: 0,
                bytes: vec![0x41],
                modified_offsets: vec![9007199254740993],
                revision: 2,
            },
        );
        let response = MinimapSamplesResponse {
            revision: "2".into(),
            samples: vec![row],
        };
        let json = serde_json::to_value(response).unwrap();
        assert_eq!(json["samples"][0]["row"], "9007199254740993");
        assert_eq!(json["samples"][0]["modifiedOffsets"][0], "9007199254740993");
    }
}
