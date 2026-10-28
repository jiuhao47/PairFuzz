pub type SemanticValue = u64;

#[derive(
    clap::ValueEnum, serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Eq, PartialEq,
)]
#[serde(rename_all = "kebab-case")]
pub enum ResultPolicy {
    ZeroNonzero,
    Exact,
}

impl ResultPolicy {
    #[must_use]
    pub const fn differs(self, a: Option<SemanticValue>, b: Option<SemanticValue>) -> bool {
        match (a, b) {
            (None, None) => false,
            (Some(_), None) | (None, Some(_)) => true,
            (Some(a), Some(b)) => match self {
                Self::ZeroNonzero => (a == 0) != (b == 0),
                Self::Exact => a != b,
            },
        }
    }

    #[must_use]
    pub const fn signature_byte(self) -> u8 {
        match self {
            Self::ZeroNonzero => 0,
            Self::Exact => 1,
        }
    }
}

#[must_use]
pub const fn legacy_value(value: i32) -> SemanticValue {
    (value as u32) as SemanticValue
}

/// Serde compatibility for values persisted before the semantic transport
/// changed from signed i32 to u64. Negative legacy values retain their exact
/// 32-bit representation through zero extension.
pub mod legacy_option {
    use super::{legacy_value, SemanticValue};
    use serde::de::{Error, Visitor};
    use serde::{Deserializer, Serializer};
    use std::fmt;

    pub fn serialize<S>(value: &Option<SemanticValue>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(value) => serializer.serialize_some(value),
            None => serializer.serialize_none(),
        }
    }

    struct OptionalSemanticVisitor;

    impl<'de> Visitor<'de> for OptionalSemanticVisitor {
        type Value = Option<SemanticValue>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("null, a u64 semantic value, or a legacy i32 value")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(SemanticVisitor).map(Some)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
            Ok(Some(value))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            SemanticVisitor.visit_i64(value).map(Some)
        }
    }

    struct SemanticVisitor;

    impl<'de> Visitor<'de> for SemanticVisitor {
        type Value = SemanticValue;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a u64 semantic value or a legacy i32 value")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: Error,
        {
            if value < 0 {
                let legacy = i32::try_from(value)
                    .map_err(|_| E::custom("negative legacy semantic value is outside i32"))?;
                Ok(legacy_value(legacy))
            } else {
                Ok(value as u64)
            }
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<SemanticValue>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_option(OptionalSemanticVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::{legacy_value, ResultPolicy, SemanticValue};

    #[test]
    fn legacy_values_are_zero_extended() {
        let value: SemanticValue = legacy_value(-1);
        assert_eq!(value, 0xffff_ffff);
    }

    #[test]
    fn semantic_policy_exact_distinguishes_full_width_values() {
        let low = Some(1_u64);
        let high = Some(0x1_0000_0001_u64);
        assert!(!ResultPolicy::ZeroNonzero.differs(low, high));
        assert!(ResultPolicy::Exact.differs(low, high));
        assert!(ResultPolicy::ZeroNonzero.differs(None, Some(1)));
        assert!(ResultPolicy::ZeroNonzero.differs(None, Some(0)));
    }
}
