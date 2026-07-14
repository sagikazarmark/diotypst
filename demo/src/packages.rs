//! The demo package allowlist and Package Policy, shared by every backend.
//!
//! The native server mounts the proxy with this policy and the Cloudflare
//! Worker enforces the same object through the shared proxy core
//! (`typst_package_source::ProxyArchiveRequest`), so the two package proxy
//! backends can never drift.

use typst_package_source::{PackagePattern, PackagePolicy};

/// Typst Universe (`@preview`) package names the demo package proxy serves:
/// CetZ and its transitive dependencies only.
pub const ALLOWED_PREVIEW_PACKAGES: &[&str] = &["cetz", "cetz-core", "oxifmt"];

/// The demo package allowlist: CetZ and its transitive dependencies, plus the
/// embedded `@demo` namespace.
pub fn demo_package_policy() -> PackagePolicy {
    let pattern = |pattern: &str| {
        pattern
            .parse::<PackagePattern>()
            .expect("demo package pattern should parse")
    };

    let mut policy = PackagePolicy::deny_all().allow(pattern("@demo/*"));
    for name in ALLOWED_PREVIEW_PACKAGES {
        policy = policy.allow(pattern(&format!("@preview/{name}")));
    }

    policy
}

#[cfg(test)]
mod tests {
    use super::demo_package_policy;

    #[test]
    fn demo_package_policy_allows_cetz_dependencies_and_denies_the_rest() {
        let policy = demo_package_policy();

        for allowed in [
            "@preview/cetz:0.4.2",
            "@preview/cetz-core:0.1.2",
            "@preview/oxifmt:1.0.0",
            "@demo/demo-badge:0.1.0",
        ] {
            assert!(
                policy.permits(&allowed.parse().expect("spec should parse")),
                "policy should allow {allowed}"
            );
        }

        assert!(!policy.permits(&"@preview/tablex:0.0.9".parse().expect("spec should parse")));
    }
}
