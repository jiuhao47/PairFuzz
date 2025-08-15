use core::fmt::{Debug, Display, Formatter};
use libafl::executors::ExitKind;
use std::fmt;

/// Extended ExitKind that can capture i32 return values from program execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffRayExitKind {
    /// Normal ExitKind variants
    Ok,
    Crash,
    Timeout,
    OomError,
    /// Extended variant that captures an i32 return value
    Value(i32),
}

impl DiffRayExitKind {
    /// Create a new DiffRayExitKind from an i32 return value
    pub fn from_return_value(value: i32) -> Self {
        match value {
            0 => DiffRayExitKind::Ok,
            _ => DiffRayExitKind::Value(value),
        }
    }

    /// Get the return value if this is a Value variant
    pub fn return_value(&self) -> Option<i32> {
        match self {
            DiffRayExitKind::Value(val) => Some(*val),
            DiffRayExitKind::Ok => Some(0),
            _ => None,
        }
    }

    /// Check if this represents a successful execution (Ok or Value(0))
    pub fn is_success(&self) -> bool {
        matches!(self, DiffRayExitKind::Ok | DiffRayExitKind::Value(0))
    }

    /// Check if this represents a failure
    pub fn is_failure(&self) -> bool {
        !self.is_success()
    }

    /// Convert to standard LibAFL ExitKind
    pub fn to_exit_kind(&self) -> ExitKind {
        match self {
            DiffRayExitKind::Ok => ExitKind::Ok,
            DiffRayExitKind::Crash => ExitKind::Crash,
            DiffRayExitKind::Timeout => ExitKind::Timeout,
            DiffRayExitKind::OomError => ExitKind::OomError,
            DiffRayExitKind::Value(0) => ExitKind::Ok,
            DiffRayExitKind::Value(_) => ExitKind::Crash, // Treat non-zero return values as crashes
        }
    }

    /// Check if two DiffRayExitKind values represent a differential behavior
    pub fn is_differential(&self, other: &DiffRayExitKind) -> bool {
        // Different types of exit kinds are always differential
        if std::mem::discriminant(self) != std::mem::discriminant(other) {
            return true;
        }

        // For Value variants, check if the return values are different
        match (self, other) {
            (DiffRayExitKind::Value(a), DiffRayExitKind::Value(b)) => a != b,
            _ => false, // Same variant, same behavior
        }
    }
}

impl From<ExitKind> for DiffRayExitKind {
    fn from(exit_kind: ExitKind) -> Self {
        match exit_kind {
            ExitKind::Ok => DiffRayExitKind::Ok,
            ExitKind::Crash => DiffRayExitKind::Crash,
            ExitKind::Timeout => DiffRayExitKind::Timeout,
            ExitKind::OomError => DiffRayExitKind::OomError,
        }
    }
}

impl From<DiffRayExitKind> for ExitKind {
    fn from(diff_exit_kind: DiffRayExitKind) -> Self {
        diff_exit_kind.to_exit_kind()
    }
}

impl From<i32> for DiffRayExitKind {
    fn from(value: i32) -> Self {
        DiffRayExitKind::from_return_value(value)
    }
}

impl Display for DiffRayExitKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            DiffRayExitKind::Ok => write!(f, "Ok"),
            DiffRayExitKind::Crash => write!(f, "Crash"),
            DiffRayExitKind::Timeout => write!(f, "Timeout"),
            DiffRayExitKind::OomError => write!(f, "OomError"),
            DiffRayExitKind::Value(val) => write!(f, "Value({})", val),
        }
    }
}

/// Helper trait for converting return values to DiffRayExitKind
pub trait IntoDiffRayExitKind {
    fn into_diff_ray_exit_kind(self) -> DiffRayExitKind;
}

impl IntoDiffRayExitKind for i32 {
    fn into_diff_ray_exit_kind(self) -> DiffRayExitKind {
        DiffRayExitKind::from_return_value(self)
    }
}

impl IntoDiffRayExitKind for ExitKind {
    fn into_diff_ray_exit_kind(self) -> DiffRayExitKind {
        DiffRayExitKind::from(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_return_value() {
        assert_eq!(DiffRayExitKind::from_return_value(0), DiffRayExitKind::Ok);
        assert_eq!(
            DiffRayExitKind::from_return_value(1),
            DiffRayExitKind::Value(1)
        );
        assert_eq!(
            DiffRayExitKind::from_return_value(-1),
            DiffRayExitKind::Value(-1)
        );
    }

    #[test]
    fn test_return_value() {
        assert_eq!(DiffRayExitKind::Ok.return_value(), Some(0));
        assert_eq!(DiffRayExitKind::Value(42).return_value(), Some(42));
        assert_eq!(DiffRayExitKind::Crash.return_value(), None);
    }

    #[test]
    fn test_is_success() {
        assert!(DiffRayExitKind::Ok.is_success());
        assert!(DiffRayExitKind::Value(0).is_success());
        assert!(!DiffRayExitKind::Value(1).is_success());
        assert!(!DiffRayExitKind::Crash.is_success());
    }

    #[test]
    fn test_to_exit_kind() {
        assert_eq!(DiffRayExitKind::Ok.to_exit_kind(), ExitKind::Ok);
        assert_eq!(DiffRayExitKind::Value(0).to_exit_kind(), ExitKind::Ok);
        assert_eq!(DiffRayExitKind::Value(1).to_exit_kind(), ExitKind::Crash);
        assert_eq!(DiffRayExitKind::Crash.to_exit_kind(), ExitKind::Crash);
    }

    #[test]
    fn test_is_differential() {
        let ok = DiffRayExitKind::Ok;
        let crash = DiffRayExitKind::Crash;
        let value1 = DiffRayExitKind::Value(1);
        let value2 = DiffRayExitKind::Value(2);
        let value1_dup = DiffRayExitKind::Value(1);

        // Different types are differential
        assert!(ok.is_differential(&crash));
        assert!(ok.is_differential(&value1));
        assert!(crash.is_differential(&value1));

        // Same types, different values are differential
        assert!(value1.is_differential(&value2));

        // Same types, same values are not differential
        assert!(!value1.is_differential(&value1_dup));
        assert!(!ok.is_differential(&ok));
    }

    #[test]
    fn test_conversions() {
        // From ExitKind
        let exit_kind = ExitKind::Ok;
        let diff_exit: DiffRayExitKind = exit_kind.into();
        assert_eq!(diff_exit, DiffRayExitKind::Ok);

        // From i32
        let diff_exit: DiffRayExitKind = 42i32.into();
        assert_eq!(diff_exit, DiffRayExitKind::Value(42));

        // To ExitKind
        let exit_kind: ExitKind = DiffRayExitKind::Value(0).into();
        assert_eq!(exit_kind, ExitKind::Ok);
    }

    #[test]
    fn test_trait_conversions() {
        assert_eq!(42i32.into_diff_ray_exit_kind(), DiffRayExitKind::Value(42));
        assert_eq!(ExitKind::Ok.into_diff_ray_exit_kind(), DiffRayExitKind::Ok);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", DiffRayExitKind::Ok), "Ok");
        assert_eq!(format!("{}", DiffRayExitKind::Crash), "Crash");
        assert_eq!(format!("{}", DiffRayExitKind::Value(42)), "Value(42)");
        assert_eq!(format!("{}", DiffRayExitKind::Value(-1)), "Value(-1)");
    }
}
