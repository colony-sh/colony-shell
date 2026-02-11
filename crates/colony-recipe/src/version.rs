//! Recipe version conflict resolution
//!
//! Implements SemVer version constraints on recipe imports and provides
//! strategies for resolving version conflicts in the dependency tree.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::error::{RecipeError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Option<String>,
}

impl SemVer {
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim().trim_start_matches('v');
        let (version_part, pre) = if let Some((v, p)) = s.split_once('-') {
            (v, Some(p.to_string()))
        } else {
            (s, None)
        };

        let parts: Vec<&str> = version_part.split('.').collect();
        if parts.len() > 3 {
            return Err(RecipeError::Invalid(format!(
                "invalid semver: too many parts in '{s}'"
            )));
        }

        let major = parts
            .first()
            .unwrap_or(&"0")
            .parse::<u64>()
            .map_err(|_| RecipeError::Invalid(format!("invalid major version in '{s}'")))?;

        let minor = parts
            .get(1)
            .unwrap_or(&"0")
            .parse::<u64>()
            .map_err(|_| RecipeError::Invalid(format!("invalid minor version in '{s}'")))?;

        let patch = parts
            .get(2)
            .unwrap_or(&"0")
            .parse::<u64>()
            .map_err(|_| RecipeError::Invalid(format!("invalid patch version in '{s}'")))?;

        Ok(Self {
            major,
            minor,
            patch,
            pre,
        })
    }
}

impl std::fmt::Display for SemVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(ref pre) = self.pre {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (Some(a), Some(b)) => a.cmp(b),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionConstraint {
    Exact(SemVer),
    Caret(SemVer),
    Tilde(SemVer),
    GreaterThanOrEqual(SemVer),
    LessThan(SemVer),
    Range {
        min: SemVer,
        max: SemVer,
    },
    Any,
}

impl VersionConstraint {
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();

        if s == "*" {
            return Ok(VersionConstraint::Any);
        }

        if let Some(rest) = s.strip_prefix("^") {
            let ver = SemVer::parse(rest)?;
            return Ok(VersionConstraint::Caret(ver));
        }

        if let Some(rest) = s.strip_prefix("~") {
            let ver = SemVer::parse(rest)?;
            return Ok(VersionConstraint::Tilde(ver));
        }

        if let Some(rest) = s.strip_prefix(">=") {
            let ver = SemVer::parse(rest)?;
            return Ok(VersionConstraint::GreaterThanOrEqual(ver));
        }

        if let Some(rest) = s.strip_prefix("<") {
            let ver = SemVer::parse(rest)?;
            return Ok(VersionConstraint::LessThan(ver));
        }

        if let Some(rest) = s.strip_prefix("=") {
            let ver = SemVer::parse(rest)?;
            return Ok(VersionConstraint::Exact(ver));
        }

        let ver = SemVer::parse(s)?;
        Ok(VersionConstraint::Caret(ver))
    }

    pub fn satisfies(&self, version: &SemVer) -> bool {
        match self {
            VersionConstraint::Any => true,
            VersionConstraint::Exact(required) => version == required,
            VersionConstraint::Caret(min) => {
                if version < min {
                    return false;
                }
                if min.major == 0 {
                    if min.minor == 0 {
                        version.major == 0
                            && version.minor == 0
                            && version.patch == min.patch
                    } else {
                        version.major == 0 && version.minor == min.minor
                    }
                } else {
                    version.major == min.major
                }
            }
            VersionConstraint::Tilde(min) => {
                if version < min {
                    return false;
                }
                version.major == min.major && version.minor == min.minor
            }
            VersionConstraint::GreaterThanOrEqual(min) => version >= min,
            VersionConstraint::LessThan(max) => version < max,
            VersionConstraint::Range { min, max } => version >= min && version < max,
        }
    }
}

impl std::fmt::Display for VersionConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exact(v) => write!(f, "={v}"),
            Self::Caret(v) => write!(f, "^{v}"),
            Self::Tilde(v) => write!(f, "~{v}"),
            Self::GreaterThanOrEqual(v) => write!(f, ">={v}"),
            Self::LessThan(v) => write!(f, "<{v}"),
            Self::Range { min, max } => write!(f, ">={min}, <{max}"),
            Self::Any => write!(f, "*"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionStrategy {
    NewestWins,
    ErrorOnConflict,
}

#[derive(Debug)]
pub struct VersionConflict {
    pub recipe_name: String,
    pub versions: Vec<VersionRequest>,
}

impl std::fmt::Display for VersionConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let reqs: Vec<String> = self
            .versions
            .iter()
            .map(|r| format!("{} (from {})", r.constraint, r.requested_by))
            .collect();
        write!(
            f,
            "version conflict for '{}': [{}]",
            self.recipe_name,
            reqs.join(", ")
        )
    }
}

#[derive(Debug, Clone)]
pub struct VersionRequest {
    pub recipe_name: String,
    pub version: SemVer,
    pub constraint: VersionConstraint,
    pub requested_by: String,
}

pub fn detect_version_conflicts(
    requests: &[VersionRequest],
) -> Vec<VersionConflict> {
    let mut by_recipe: HashMap<&str, Vec<&VersionRequest>> = HashMap::new();
    for req in requests {
        by_recipe
            .entry(&req.recipe_name)
            .or_default()
            .push(req);
    }

    let mut conflicts = Vec::new();

    for (name, reqs) in &by_recipe {
        if reqs.len() <= 1 {
            continue;
        }

        let has_conflict = reqs.iter().any(|r1| {
            reqs.iter().any(|r2| {
                !std::ptr::eq(*r1, *r2) && !r1.constraint.satisfies(&r2.version)
            })
        });

        if has_conflict {
            conflicts.push(VersionConflict {
                recipe_name: name.to_string(),
                versions: reqs.iter().map(|r| (*r).clone()).collect(),
            });
        }
    }

    conflicts
}

pub fn resolve_version(
    conflict: &VersionConflict,
    strategy: &ResolutionStrategy,
) -> Result<SemVer> {
    match strategy {
        ResolutionStrategy::NewestWins => {
            let newest = conflict
                .versions
                .iter()
                .map(|r| &r.version)
                .max()
                .ok_or_else(|| {
                    RecipeError::Invalid(format!(
                        "no versions found for '{}'",
                        conflict.recipe_name
                    ))
                })?;
            Ok(newest.clone())
        }
        ResolutionStrategy::ErrorOnConflict => Err(RecipeError::Invalid(format!(
            "{}",
            conflict
        ))),
    }
}

pub fn parse_version_import(import_str: &str) -> Result<(String, VersionConstraint)> {
    let trimmed = import_str.trim();

    if let Some((name, constraint_str)) = trimmed.split_once('@') {
        let constraint = VersionConstraint::parse(constraint_str)?;
        Ok((name.to_string(), constraint))
    } else {
        Ok((trimmed.to_string(), VersionConstraint::Any))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_parse_full() {
        let v = SemVer::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(v.pre.is_none());
    }

    #[test]
    fn test_semver_parse_major_minor() {
        let v = SemVer::parse("1.2").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_semver_parse_major_only() {
        let v = SemVer::parse("1").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_semver_parse_prerelease() {
        let v = SemVer::parse("1.0.0-beta.1").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.pre, Some("beta.1".to_string()));
    }

    #[test]
    fn test_semver_parse_v_prefix() {
        let v = SemVer::parse("v2.3.4").unwrap();
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 3);
        assert_eq!(v.patch, 4);
    }

    #[test]
    fn test_semver_parse_invalid() {
        assert!(SemVer::parse("not.a.number").is_err());
    }

    #[test]
    fn test_semver_ordering() {
        let v100 = SemVer::parse("1.0.0").unwrap();
        let v110 = SemVer::parse("1.1.0").unwrap();
        let v200 = SemVer::parse("2.0.0").unwrap();
        let v101 = SemVer::parse("1.0.1").unwrap();

        assert!(v100 < v110);
        assert!(v110 < v200);
        assert!(v100 < v101);
        assert!(v101 < v110);
    }

    #[test]
    fn test_semver_prerelease_ordering() {
        let stable = SemVer::parse("1.0.0").unwrap();
        let beta = SemVer::parse("1.0.0-beta").unwrap();
        assert!(beta < stable);
    }

    #[test]
    fn test_semver_display() {
        assert_eq!(SemVer::parse("1.2.3").unwrap().to_string(), "1.2.3");
        assert_eq!(
            SemVer::parse("1.0.0-beta").unwrap().to_string(),
            "1.0.0-beta"
        );
    }

    #[test]
    fn test_constraint_parse_caret() {
        let c = VersionConstraint::parse("^1.2.0").unwrap();
        assert!(matches!(c, VersionConstraint::Caret(_)));
    }

    #[test]
    fn test_constraint_parse_tilde() {
        let c = VersionConstraint::parse("~1.2.0").unwrap();
        assert!(matches!(c, VersionConstraint::Tilde(_)));
    }

    #[test]
    fn test_constraint_parse_exact() {
        let c = VersionConstraint::parse("=1.2.3").unwrap();
        assert!(matches!(c, VersionConstraint::Exact(_)));
    }

    #[test]
    fn test_constraint_parse_gte() {
        let c = VersionConstraint::parse(">=1.0.0").unwrap();
        assert!(matches!(c, VersionConstraint::GreaterThanOrEqual(_)));
    }

    #[test]
    fn test_constraint_parse_lt() {
        let c = VersionConstraint::parse("<2.0.0").unwrap();
        assert!(matches!(c, VersionConstraint::LessThan(_)));
    }

    #[test]
    fn test_constraint_parse_star() {
        let c = VersionConstraint::parse("*").unwrap();
        assert_eq!(c, VersionConstraint::Any);
    }

    #[test]
    fn test_constraint_parse_bare_version() {
        let c = VersionConstraint::parse("1.2.0").unwrap();
        assert!(matches!(c, VersionConstraint::Caret(_)));
    }

    #[test]
    fn test_caret_satisfies() {
        let c = VersionConstraint::Caret(SemVer::parse("1.2.0").unwrap());

        assert!(c.satisfies(&SemVer::parse("1.2.0").unwrap()));
        assert!(c.satisfies(&SemVer::parse("1.3.0").unwrap()));
        assert!(c.satisfies(&SemVer::parse("1.99.99").unwrap()));
        assert!(!c.satisfies(&SemVer::parse("2.0.0").unwrap()));
        assert!(!c.satisfies(&SemVer::parse("1.1.0").unwrap()));
    }

    #[test]
    fn test_caret_zero_major() {
        let c = VersionConstraint::Caret(SemVer::parse("0.2.0").unwrap());

        assert!(c.satisfies(&SemVer::parse("0.2.0").unwrap()));
        assert!(c.satisfies(&SemVer::parse("0.2.5").unwrap()));
        assert!(!c.satisfies(&SemVer::parse("0.3.0").unwrap()));
        assert!(!c.satisfies(&SemVer::parse("1.0.0").unwrap()));
    }

    #[test]
    fn test_caret_zero_zero() {
        let c = VersionConstraint::Caret(SemVer::parse("0.0.3").unwrap());

        assert!(c.satisfies(&SemVer::parse("0.0.3").unwrap()));
        assert!(!c.satisfies(&SemVer::parse("0.0.4").unwrap()));
        assert!(!c.satisfies(&SemVer::parse("0.1.0").unwrap()));
    }

    #[test]
    fn test_tilde_satisfies() {
        let c = VersionConstraint::Tilde(SemVer::parse("1.2.0").unwrap());

        assert!(c.satisfies(&SemVer::parse("1.2.0").unwrap()));
        assert!(c.satisfies(&SemVer::parse("1.2.9").unwrap()));
        assert!(!c.satisfies(&SemVer::parse("1.3.0").unwrap()));
        assert!(!c.satisfies(&SemVer::parse("1.1.0").unwrap()));
    }

    #[test]
    fn test_exact_satisfies() {
        let c = VersionConstraint::Exact(SemVer::parse("1.2.3").unwrap());

        assert!(c.satisfies(&SemVer::parse("1.2.3").unwrap()));
        assert!(!c.satisfies(&SemVer::parse("1.2.4").unwrap()));
    }

    #[test]
    fn test_gte_satisfies() {
        let c = VersionConstraint::GreaterThanOrEqual(SemVer::parse("1.0.0").unwrap());

        assert!(c.satisfies(&SemVer::parse("1.0.0").unwrap()));
        assert!(c.satisfies(&SemVer::parse("2.0.0").unwrap()));
        assert!(!c.satisfies(&SemVer::parse("0.9.0").unwrap()));
    }

    #[test]
    fn test_lt_satisfies() {
        let c = VersionConstraint::LessThan(SemVer::parse("2.0.0").unwrap());

        assert!(c.satisfies(&SemVer::parse("1.9.9").unwrap()));
        assert!(!c.satisfies(&SemVer::parse("2.0.0").unwrap()));
    }

    #[test]
    fn test_any_satisfies() {
        let c = VersionConstraint::Any;
        assert!(c.satisfies(&SemVer::parse("0.0.0").unwrap()));
        assert!(c.satisfies(&SemVer::parse("99.99.99").unwrap()));
    }

    #[test]
    fn test_range_satisfies() {
        let c = VersionConstraint::Range {
            min: SemVer::parse("1.0.0").unwrap(),
            max: SemVer::parse("2.0.0").unwrap(),
        };

        assert!(c.satisfies(&SemVer::parse("1.0.0").unwrap()));
        assert!(c.satisfies(&SemVer::parse("1.9.9").unwrap()));
        assert!(!c.satisfies(&SemVer::parse("0.9.9").unwrap()));
        assert!(!c.satisfies(&SemVer::parse("2.0.0").unwrap()));
    }

    #[test]
    fn test_detect_no_conflicts() {
        let requests = vec![
            VersionRequest {
                recipe_name: "base".to_string(),
                version: SemVer::parse("1.0.0").unwrap(),
                constraint: VersionConstraint::Caret(SemVer::parse("1.0.0").unwrap()),
                requested_by: "a".to_string(),
            },
            VersionRequest {
                recipe_name: "base".to_string(),
                version: SemVer::parse("1.2.0").unwrap(),
                constraint: VersionConstraint::Caret(SemVer::parse("1.0.0").unwrap()),
                requested_by: "b".to_string(),
            },
        ];

        let conflicts = detect_version_conflicts(&requests);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_detect_conflict() {
        let requests = vec![
            VersionRequest {
                recipe_name: "base".to_string(),
                version: SemVer::parse("1.0.0").unwrap(),
                constraint: VersionConstraint::Exact(SemVer::parse("1.0.0").unwrap()),
                requested_by: "a".to_string(),
            },
            VersionRequest {
                recipe_name: "base".to_string(),
                version: SemVer::parse("2.0.0").unwrap(),
                constraint: VersionConstraint::Exact(SemVer::parse("2.0.0").unwrap()),
                requested_by: "b".to_string(),
            },
        ];

        let conflicts = detect_version_conflicts(&requests);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].recipe_name, "base");
    }

    #[test]
    fn test_resolve_newest_wins() {
        let conflict = VersionConflict {
            recipe_name: "base".to_string(),
            versions: vec![
                VersionRequest {
                    recipe_name: "base".to_string(),
                    version: SemVer::parse("1.0.0").unwrap(),
                    constraint: VersionConstraint::Any,
                    requested_by: "a".to_string(),
                },
                VersionRequest {
                    recipe_name: "base".to_string(),
                    version: SemVer::parse("2.0.0").unwrap(),
                    constraint: VersionConstraint::Any,
                    requested_by: "b".to_string(),
                },
            ],
        };

        let resolved = resolve_version(&conflict, &ResolutionStrategy::NewestWins).unwrap();
        assert_eq!(resolved, SemVer::parse("2.0.0").unwrap());
    }

    #[test]
    fn test_resolve_error_on_conflict() {
        let conflict = VersionConflict {
            recipe_name: "base".to_string(),
            versions: vec![VersionRequest {
                recipe_name: "base".to_string(),
                version: SemVer::parse("1.0.0").unwrap(),
                constraint: VersionConstraint::Any,
                requested_by: "a".to_string(),
            }],
        };

        let result = resolve_version(&conflict, &ResolutionStrategy::ErrorOnConflict);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_version_import_with_constraint() {
        let (name, constraint) = parse_version_import("base@^1.0.0").unwrap();
        assert_eq!(name, "base");
        assert!(matches!(constraint, VersionConstraint::Caret(_)));
    }

    #[test]
    fn test_parse_version_import_without_constraint() {
        let (name, constraint) = parse_version_import("base").unwrap();
        assert_eq!(name, "base");
        assert_eq!(constraint, VersionConstraint::Any);
    }

    #[test]
    fn test_parse_version_import_exact() {
        let (name, constraint) = parse_version_import("tools@=2.0.0").unwrap();
        assert_eq!(name, "tools");
        assert!(matches!(constraint, VersionConstraint::Exact(_)));
    }

    #[test]
    fn test_constraint_display() {
        assert_eq!(
            VersionConstraint::Caret(SemVer::parse("1.0.0").unwrap()).to_string(),
            "^1.0.0"
        );
        assert_eq!(VersionConstraint::Any.to_string(), "*");
        assert_eq!(
            VersionConstraint::Exact(SemVer::parse("1.2.3").unwrap()).to_string(),
            "=1.2.3"
        );
    }

    #[test]
    fn test_version_conflict_display() {
        let conflict = VersionConflict {
            recipe_name: "base".to_string(),
            versions: vec![
                VersionRequest {
                    recipe_name: "base".to_string(),
                    version: SemVer::parse("1.0.0").unwrap(),
                    constraint: VersionConstraint::Caret(SemVer::parse("1.0.0").unwrap()),
                    requested_by: "a".to_string(),
                },
            ],
        };

        let display = conflict.to_string();
        assert!(display.contains("base"));
        assert!(display.contains("^1.0.0"));
    }
}
