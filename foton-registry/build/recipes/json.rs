//! JSON shapes and strict file loading for extracted vanilla recipes.
#![cfg_attr(
    test,
    expect(
        dead_code,
        reason = "integration test exercises parser failures without generating recipes"
    )
)]

use std::{fs, path::Path};

use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, Debug)]
pub(super) struct RecipeJson {
    #[serde(rename = "type")]
    pub(super) recipe_type: String,
    #[serde(default)]
    pub(super) category: Option<String>,
    #[serde(default)]
    pub(super) key: Option<serde_json::Map<String, Value>>,
    #[serde(default)]
    pub(super) pattern: Option<RecipePattern>,
    #[serde(default)]
    pub(super) ingredients: Option<Vec<Value>>,
    #[serde(default)]
    pub(super) ingredient: Option<Value>,
    #[serde(default)]
    pub(super) template: Option<Value>,
    #[serde(default)]
    pub(super) base: Option<Value>,
    #[serde(default)]
    pub(super) addition: Option<Value>,
    #[serde(default)]
    pub(super) cookingtime: Option<i32>,
    #[serde(default)]
    pub(super) experience: Option<f32>,
    #[serde(default)]
    pub(super) result: Option<RecipeResult>,
    #[serde(default)]
    pub(super) show_notification: Option<bool>,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub(super) enum RecipePattern {
    Grid(Vec<String>),
    #[expect(
        dead_code,
        reason = "smithing trim pattern is parsed but trim recipes are unsupported"
    )]
    SmithingTrim(String),
}

#[derive(Deserialize, Debug)]
pub(super) struct RecipeResult {
    pub(super) id: String,
    #[serde(default = "default_count")]
    pub(super) count: i32,
    #[serde(default)]
    components: Option<serde_json::Map<String, Value>>,
}

impl RecipeResult {
    pub(super) fn components(&self) -> Option<serde_json::Map<String, Value>> {
        self.components.clone()
    }
}

const fn default_count() -> i32 {
    1
}

pub(super) fn read_recipe_json(path: &Path) -> Result<RecipeJson, String> {
    let content =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_str(&content).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::read_recipe_json;
    use std::path::Path;

    #[test]
    fn malformed_extracted_result_components_report_the_source_file() {
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/malformed_recipe_result_components.json"
        ));
        let error = read_recipe_json(path).expect_err("invalid components shape must fail");
        assert!(error.contains(&path.display().to_string()));
        assert!(error.contains("components"));
        assert!(error.contains("expected a map"));
    }
}
