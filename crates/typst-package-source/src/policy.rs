use crate::PackageSpec;
use std::fmt;
use std::str::FromStr;
use typst_syntax::package::PackageVersion;

/// A pattern matching Typst packages at namespace, name, or exact-version granularity.
///
/// Patterns parse from text: `@preview/*` matches a whole namespace, `@preview/cetz` matches
/// every version of one package, and `@preview/cetz:0.4.2` matches one exact Package Spec.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackagePattern {
    /// Match every package in a namespace, such as `@preview/*`.
    Namespace { namespace: String },

    /// Match every version of one package, such as `@preview/cetz`.
    Name { namespace: String, name: String },

    /// Match one exact Package Spec, such as `@preview/cetz:0.4.2`.
    Exact {
        namespace: String,
        name: String,
        version: PackageVersion,
    },
}

impl PackagePattern {
    /// Return whether this pattern matches the given exact Package Spec.
    pub fn matches(&self, spec: &PackageSpec) -> bool {
        match self {
            Self::Namespace { namespace } => spec.namespace == namespace.as_str(),
            Self::Name { namespace, name } => {
                spec.namespace == namespace.as_str() && spec.name == name.as_str()
            }
            Self::Exact {
                namespace,
                name,
                version,
            } => {
                spec.namespace == namespace.as_str()
                    && spec.name == name.as_str()
                    && spec.version == *version
            }
        }
    }
}

impl From<&PackageSpec> for PackagePattern {
    fn from(spec: &PackageSpec) -> Self {
        Self::Exact {
            namespace: spec.namespace.to_string(),
            name: spec.name.to_string(),
            version: spec.version,
        }
    }
}

impl FromStr for PackagePattern {
    type Err = PackagePatternError;

    fn from_str(pattern: &str) -> Result<Self, Self::Err> {
        let invalid = || PackagePatternError::Invalid {
            pattern: pattern.to_owned(),
        };

        let rest = pattern.strip_prefix('@').ok_or_else(invalid)?;
        let (namespace, rest) = rest.split_once('/').ok_or_else(invalid)?;

        if namespace.is_empty() || namespace.contains([':', '*']) {
            return Err(invalid());
        }

        if rest == "*" {
            return Ok(Self::Namespace {
                namespace: namespace.to_owned(),
            });
        }

        if rest.contains(':') {
            // Exact patterns must be valid exact Package Specs.
            let spec = PackageSpec::from_str(pattern).map_err(|_| invalid())?;

            return Ok(Self::from(&spec));
        }

        if rest.is_empty() || rest.contains(['/', '*']) {
            return Err(invalid());
        }

        Ok(Self::Name {
            namespace: namespace.to_owned(),
            name: rest.to_owned(),
        })
    }
}

impl fmt::Display for PackagePattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Namespace { namespace } => write!(f, "@{namespace}/*"),
            Self::Name { namespace, name } => write!(f, "@{namespace}/{name}"),
            Self::Exact {
                namespace,
                name,
                version,
            } => write!(f, "@{namespace}/{name}:{version}"),
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for PackagePattern {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for PackagePattern {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let pattern = <String as serde::Deserialize>::deserialize(deserializer)?;

        pattern
            .parse()
            .map_err(|_| serde::de::Error::custom("invalid package pattern"))
    }
}

/// A package pattern parsing failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackagePatternError {
    /// The pattern is not a namespace, name, or exact Package Spec pattern.
    Invalid { pattern: String },
}

/// An explicit allowlist/denylist deciding which packages a Package Source may resolve.
///
/// Deny patterns win over allow patterns; anything unmatched falls back to the policy default
/// chosen at construction ([`PackagePolicy::allow_all`] or [`PackagePolicy::deny_all`]).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackagePolicy {
    default_allow: bool,
    allow: Vec<PackagePattern>,
    deny: Vec<PackagePattern>,
}

impl PackagePolicy {
    /// Create a policy that permits every package not explicitly denied.
    pub fn allow_all() -> Self {
        Self {
            default_allow: true,
            allow: Vec::new(),
            deny: Vec::new(),
        }
    }

    /// Create a policy that denies every package not explicitly allowed.
    pub fn deny_all() -> Self {
        Self {
            default_allow: false,
            allow: Vec::new(),
            deny: Vec::new(),
        }
    }

    /// Explicitly allow packages matching a pattern.
    pub fn allow(mut self, pattern: PackagePattern) -> Self {
        self.allow.push(pattern);
        self
    }

    /// Explicitly deny packages matching a pattern; deny patterns win over allow patterns.
    pub fn deny(mut self, pattern: PackagePattern) -> Self {
        self.deny.push(pattern);
        self
    }

    /// Return whether this policy permits resolving the given exact Package Spec.
    pub fn permits(&self, spec: &PackageSpec) -> bool {
        if self.deny.iter().any(|pattern| pattern.matches(spec)) {
            return false;
        }

        if self.allow.iter().any(|pattern| pattern.matches(spec)) {
            return true;
        }

        self.default_allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(spec: &str) -> PackageSpec {
        spec.parse().expect("test spec should parse")
    }

    #[test]
    fn pattern_parses_all_granularities() {
        assert_eq!(
            "@preview/*".parse::<PackagePattern>(),
            Ok(PackagePattern::Namespace {
                namespace: "preview".to_owned()
            })
        );
        assert_eq!(
            "@preview/cetz".parse::<PackagePattern>(),
            Ok(PackagePattern::Name {
                namespace: "preview".to_owned(),
                name: "cetz".to_owned()
            })
        );
        assert_eq!(
            "@preview/cetz:0.4.2".parse::<PackagePattern>(),
            Ok(PackagePattern::Exact {
                namespace: "preview".to_owned(),
                name: "cetz".to_owned(),
                version: "0.4.2".parse().expect("test version should parse")
            })
        );
    }

    #[test]
    fn pattern_rejects_invalid_forms() {
        for pattern in [
            "preview/cetz",
            "@preview",
            "@/cetz",
            "@preview/",
            "@preview/ce*tz",
            "@preview/cetz:latest",
            "@pre:view/*",
        ] {
            assert!(
                pattern.parse::<PackagePattern>().is_err(),
                "pattern should be rejected: {pattern}"
            );
        }
    }

    #[test]
    fn pattern_round_trips_through_display() {
        for pattern in ["@preview/*", "@preview/cetz", "@preview/cetz:0.4.2"] {
            let parsed: PackagePattern = pattern.parse().expect("pattern should parse");
            assert_eq!(parsed.to_string(), pattern);
        }
    }

    #[test]
    fn pattern_matches_by_granularity() {
        let cetz = spec("@preview/cetz:0.4.2");
        let other_version = spec("@preview/cetz:0.3.0");
        let other_name = spec("@preview/oxifmt:1.0.0");
        let other_namespace = spec("@local/cetz:0.4.2");

        let namespace: PackagePattern = "@preview/*".parse().unwrap();
        assert!(namespace.matches(&cetz));
        assert!(namespace.matches(&other_name));
        assert!(!namespace.matches(&other_namespace));

        let name: PackagePattern = "@preview/cetz".parse().unwrap();
        assert!(name.matches(&cetz));
        assert!(name.matches(&other_version));
        assert!(!name.matches(&other_name));

        let exact: PackagePattern = "@preview/cetz:0.4.2".parse().unwrap();
        assert!(exact.matches(&cetz));
        assert!(!exact.matches(&other_version));
    }

    #[test]
    fn policy_deny_wins_over_allow() {
        let policy = PackagePolicy::allow_all()
            .allow("@preview/cetz".parse().unwrap())
            .deny("@preview/cetz:0.4.2".parse().unwrap());

        assert!(policy.permits(&spec("@preview/cetz:0.3.0")));
        assert!(!policy.permits(&spec("@preview/cetz:0.4.2")));
    }

    #[test]
    fn policy_defaults_apply_to_unmatched_specs() {
        let allow_all = PackagePolicy::allow_all();
        assert!(allow_all.permits(&spec("@preview/cetz:0.4.2")));

        let deny_all = PackagePolicy::deny_all();
        assert!(!deny_all.permits(&spec("@preview/cetz:0.4.2")));

        let allowlist = PackagePolicy::deny_all().allow("@preview/cetz".parse().unwrap());
        assert!(allowlist.permits(&spec("@preview/cetz:0.4.2")));
        assert!(!allowlist.permits(&spec("@preview/oxifmt:1.0.0")));
    }
}
