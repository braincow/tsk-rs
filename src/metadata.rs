use std::str::FromStr;

use thiserror::Error;

/// Errors that can occur during metadata handling
#[derive(Error, Debug, PartialEq, Eq)]
pub enum MetadataKeyValuePairError {
    /// Malformed key=value pair was given as an input
    #[error("error on parsing key=value pair")]
    ParseError,
    /// Missing constraint: metadata key names must begin with "x-" prefix
    #[error("metadata key must start with 'x-' prefix")]
    InvalidPrefix,
}

/// Key value pair for metadata storage
#[derive(Clone, Debug)]
pub struct MetadataKeyValuePair {
    /// Key of the metadata value
    pub key: String,
    /// Value of the metadata
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_success() {
        let metadata = MetadataKeyValuePair::from_str("x-fuu=bar").unwrap();
        assert_eq!(metadata.key, "x-fuu");
        assert_eq!(metadata.value, "bar");
    }

    #[test]
    fn test_parse_notpair_failure() {
        let metadata = MetadataKeyValuePair::from_str("x-fuubar");
        assert_eq!(
            metadata.err().unwrap(),
            MetadataKeyValuePairError::ParseError
        );
    }

    #[test]
    fn test_parse_prefix_failure() {
        let metadata = MetadataKeyValuePair::from_str("fuu=bar");
        assert_eq!(
            metadata.err().unwrap(),
            MetadataKeyValuePairError::InvalidPrefix
        );
    }
}
