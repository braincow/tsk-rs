use std::str::FromStr;

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum MetadataKeyValuePairError {
    #[error("error on parsing key=value pair")]
    ParseError,
    #[error("metadata key must start with 'x-' prefix")]
    InvalidPrefix,
}

#[derive(Clone, Debug)]
pub struct MetadataKeyValuePair {
    pub key: String,
    pub value: String,
}

impl FromStr for MetadataKeyValuePair {
    type Err = MetadataKeyValuePairError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts = s.split('=').collect::<Vec<&str>>();
        if parts.len() != 2 {
            return Err(MetadataKeyValuePairError::ParseError);
        }
    
        if !parts[0].starts_with("x-") {
            return Err(MetadataKeyValuePairError::InvalidPrefix);
        }

        Ok(Self {
            key: parts[0].to_string(),
            value: parts[1].to_string(),
        })
    }
}
