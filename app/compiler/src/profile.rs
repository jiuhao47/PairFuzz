use std::env::VarError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetProfile {
    Asan,
    Release,
}

impl TargetProfile {
    pub fn from_env() -> Result<Self, String> {
        match std::env::var("PAIRFUZZ_TARGET_PROFILE") {
            Err(VarError::NotPresent) => Ok(Self::Asan),
            Ok(value) if value == "asan" => Ok(Self::Asan),
            Ok(value) if value == "release" => Ok(Self::Release),
            Ok(value) => Err(format!(
                "invalid PAIRFUZZ_TARGET_PROFILE={value:?}; expected `asan` or `release`"
            )),
            Err(error) => Err(format!(
                "invalid PAIRFUZZ_TARGET_PROFILE value: {error}; expected `asan` or `release`"
            )),
        }
    }

    pub const fn uses_asan(self) -> bool {
        matches!(self, Self::Asan)
    }
}

#[cfg(test)]
mod tests {
    use super::TargetProfile;

    #[test]
    fn only_asan_profile_enables_address_sanitizer() {
        assert!(TargetProfile::Asan.uses_asan());
        assert!(!TargetProfile::Release.uses_asan());
    }
}
