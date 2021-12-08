use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub struct Metadata {
    pub name: Option<String>,
    pub description: Option<String>,
}

pub type Extension = Option<Metadata>;
