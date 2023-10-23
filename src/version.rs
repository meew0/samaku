// Passed as an environment variable by build.rs
const GIT_HASH: &str = env!("GIT_HASH");
const CARGO_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Writes only the crate version number when `Display`ed.
pub struct Short;

impl std::fmt::Display for Short {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(CARGO_VERSION)
    }
}

/// Writes the crate version number as well as the first characters of the Git hash.
pub struct Long;

impl std::fmt::Display for Long {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}-{}", CARGO_VERSION, &GIT_HASH[0..9])
    }
}
