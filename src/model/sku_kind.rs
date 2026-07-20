//! [`SkuKind`].

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkuKind {
    Simple,
    Composite,
}

impl SkuKind {
    /// Wire/storage spelling, used for the `catalog.skus.kind` column, form
    /// values, and the rendered label source.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Composite => "composite",
        }
    }

    /// Human-readable label for the SKU table.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Simple => "Simple",
            Self::Composite => "Composite",
        }
    }
}

impl fmt::Display for SkuKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SkuKind {
    type Err = String;

    /// Parse a kind, rejecting anything but `simple`/`composite` (case- and
    /// whitespace-insensitive). Never defaults silently.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "simple" => Ok(Self::Simple),
            "composite" => Ok(Self::Composite),
            other => Err(format!("invalid kind: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_kinds_case_insensitively() {
        assert_eq!(
            " Composite ".parse::<SkuKind>().unwrap(),
            SkuKind::Composite
        );
        assert_eq!("simple".parse::<SkuKind>().unwrap(), SkuKind::Simple);
    }

    #[test]
    fn rejects_unknown_kind_instead_of_defaulting() {
        let err = "widget".parse::<SkuKind>().unwrap_err();
        assert!(err.contains("widget"));
    }

    #[test]
    fn as_str_round_trips() {
        for kind in [SkuKind::Simple, SkuKind::Composite] {
            assert_eq!(kind.as_str().parse::<SkuKind>().unwrap(), kind);
        }
    }
}
