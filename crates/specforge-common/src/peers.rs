//! The diagnostics of the peer rule (ADR 0041): E073 for a range that can't be read, E027 for a
//! peer left unsatisfied or a cycle among required peers. The registry build and the add/update
//! gate build them here, so each message and suggestion is written once.

use crate::{Diagnostic, codes};
use specforge_package::peers::{UnreadableRange, Verdict};
use specforge_protocol_types::PeerDependency;

/// What `verdict` reports about `dependent`'s `declared` peer: `None` when it is satisfied.
pub fn of(dependent: &str, declared: &PeerDependency, verdict: &Verdict) -> Option<Diagnostic> {
    let peer = &declared.name;
    let range = &declared.version;
    match verdict {
        Verdict::Satisfied => None,
        Verdict::Missing => Some(
            Diagnostic::new(
                codes::E027,
                format!(
                    "extension '{dependent}' requires peer dependency '{peer}' {range} which is not installed"
                ),
            )
            .with_suggestion(format!("install it with: specforge add {peer}")),
        ),
        Verdict::OutOfRange { installed } => Some(
            Diagnostic::new(
                codes::E027,
                format!(
                    "extension '{dependent}' requires peer dependency '{peer}' {range} but version {installed} is installed"
                ),
            )
            .with_suggestion(format!(
                "install a version of '{peer}' that {range} accepts, or a version of '{dependent}' that accepts {installed}"
            )),
        ),
        Verdict::NotSemver { installed } => Some(
            Diagnostic::new(
                codes::E027,
                format!(
                    "extension '{dependent}' requires peer dependency '{peer}' {range} but version '{installed}' is installed, which is not SemVer"
                ),
            )
            .with_suggestion(format!(
                "no range accepts a version that is not SemVer: install a build of '{peer}' that declares one (such as 1.0.0)"
            )),
        ),
        Verdict::Unreadable(why) => Some(unreadable(dependent, declared, why)),
    }
}

/// E073: `dependent` declares a peer whose range is not a SemVer requirement.
pub fn unreadable(dependent: &str, declared: &PeerDependency, why: &UnreadableRange) -> Diagnostic {
    Diagnostic::new(
        codes::E073,
        format!(
            "extension '{dependent}' declares peer dependency '{}' with range '{}', which is not a SemVer requirement: {}",
            declared.name, declared.version, why.reason
        ),
    )
    .with_suggestion(format!(
        "no version can satisfy it: install a version of '{dependent}' that declares a SemVer range (^1.0.0, ~1.2.0, >=1.0.0), or report it to its author"
    ))
}

/// E027: required peers that require each other, `members` in entry order.
pub fn cycle(members: &[&str]) -> Diagnostic {
    Diagnostic::new(
        codes::E027,
        format!(
            "cycle detected in peer dependencies: {}",
            members.join(", ")
        ),
    )
    .with_suggestion(
        "make one of these peer dependencies optional, or remove it: required peers that require each other can't load dependencies first"
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Severity;
    use specforge_package::peers::verdict;

    fn declared(range: &str) -> PeerDependency {
        PeerDependency {
            name: "@t/base".into(),
            version: range.into(),
            optional: false,
        }
    }

    fn report(range: &str, installed: Option<&str>) -> Option<Diagnostic> {
        let declared = declared(range);
        of("@t/dep", &declared, &verdict(&declared, installed))
    }

    #[test]
    fn each_verdict_has_its_code_and_suggestion() {
        assert!(report("^1.0", Some("1.2.0")).is_none());

        let d = report("^1.0", None).unwrap();
        assert_eq!(d.code, "E027");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(
            d.message,
            "extension '@t/dep' requires peer dependency '@t/base' ^1.0 which is not installed"
        );
        assert_eq!(
            d.suggestion.as_deref(),
            Some("install it with: specforge add @t/base")
        );

        let d = report("^1.0", Some("2.0.0")).unwrap();
        assert_eq!(d.code, "E027");
        assert_eq!(
            d.message,
            "extension '@t/dep' requires peer dependency '@t/base' ^1.0 but version 2.0.0 is installed"
        );
        assert_eq!(
            d.suggestion.as_deref(),
            Some(
                "install a version of '@t/base' that ^1.0 accepts, or a version of '@t/dep' that accepts 2.0.0"
            )
        );

        let d = report("^1.0", Some("local")).unwrap();
        assert_eq!(d.code, "E027");
        assert_eq!(
            d.message,
            "extension '@t/dep' requires peer dependency '@t/base' ^1.0 but version 'local' is installed, which is not SemVer"
        );
        assert_eq!(
            d.suggestion.as_deref(),
            Some(
                "no range accepts a version that is not SemVer: install a build of '@t/base' that declares one (such as 1.0.0)"
            )
        );

        let d = report("one-ish", Some("1.0.0")).unwrap();
        assert_eq!(d.code, "E073");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(
            d.message,
            "extension '@t/dep' declares peer dependency '@t/base' with range 'one-ish', which is not a SemVer requirement: unexpected character 'o' while parsing major version number"
        );
        assert_eq!(
            d.suggestion.as_deref(),
            Some(
                "no version can satisfy it: install a version of '@t/dep' that declares a SemVer range (^1.0.0, ~1.2.0, >=1.0.0), or report it to its author"
            )
        );

        let d = cycle(&["@t/cyca", "@t/cycb"]);
        assert_eq!(d.code, "E027");
        assert_eq!(
            d.message,
            "cycle detected in peer dependencies: @t/cyca, @t/cycb"
        );
        assert_eq!(
            d.suggestion.as_deref(),
            Some(
                "make one of these peer dependencies optional, or remove it: required peers that require each other can't load dependencies first"
            )
        );
    }
}
