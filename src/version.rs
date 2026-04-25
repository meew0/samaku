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
        let rev = if GIT_HASH.is_ascii() {
            #[expect(
                clippy::string_slice,
                reason = "safe because we assure the string is ASCII only"
            )]
            let rev = &GIT_HASH[0..9];
            rev
        } else {
            "unknown"
        };
        write!(formatter, "{CARGO_VERSION}-{rev}")
    }
}
