use super::error::without_project;
use super::*;
use crate::lifecycle::Revision;
use crate::state::McpState;
use crate::tool::{ErrorCode, McpError};
use std::path::PathBuf;

#[specforge_test_macros::test(
    behavior = "list_mcp_tools",
    verify = "a tool's path is declared once, by its target"
)]
fn each_input_makes_its_target() {
    let project = |reach, without| TargetSpec::Project { reach, without };
    assert_eq!(Unscoped::TARGET, TargetSpec::Unscoped);
    assert_eq!(NewProject::TARGET, TargetSpec::NewProject);
    assert_eq!(
        View::<Served>::TARGET,
        project(Reach::Served, WithoutProject::EmptySession)
    );
    assert_eq!(
        View::<AnyProject>::TARGET,
        project(Reach::AnyProject, WithoutProject::EmptySession)
    );
    assert_eq!(
        Project::<Served>::TARGET,
        project(Reach::Served, WithoutProject::Refused)
    );
    assert_eq!(
        Project::<AnyProject>::TARGET,
        project(Reach::AnyProject, WithoutProject::Refused)
    );
}

#[specforge_test_macros::test(
    behavior = "list_mcp_tools",
    verify = "a tool's path is declared once, by its target"
)]
fn target_arguments_follow_reach() {
    let names = |spec: TargetSpec| -> Vec<String> { spec.properties().keys().cloned().collect() };
    let mut specs = vec![TargetSpec::Unscoped, TargetSpec::NewProject];
    for reach in [Reach::Served, Reach::AnyProject] {
        for without in [WithoutProject::EmptySession, WithoutProject::Refused] {
            specs.push(TargetSpec::Project { reach, without });
        }
    }
    for spec in specs {
        let takes_path = match spec {
            TargetSpec::Unscoped => false,
            TargetSpec::NewProject => true,
            TargetSpec::Project { reach, .. } => reach == Reach::AnyProject,
        };
        let mut declared = names(spec);
        declared.sort();
        let mut fields: Vec<String> = spec
            .fields()
            .iter()
            .map(|argument| argument.name().to_string())
            .collect();
        fields.sort();
        assert_eq!(declared, fields, "{spec:?}");
        assert_eq!(
            declared.iter().any(|n| n == TargetArgument::Path.name()),
            takes_path,
            "{spec:?}"
        );
        assert!(
            declared.iter().all(|n| n != "use_cached"),
            "{spec:?} lists use_cached"
        );
        assert_eq!(
            spec.required(),
            if spec == TargetSpec::NewProject {
                &[TargetArgument::Path][..]
            } else {
                &[][..]
            },
            "{spec:?}"
        );
        assert_eq!(spec.takes_path(), takes_path, "{spec:?}");
    }
    // `path` reads differently where the call creates its project.
    let described = |spec: TargetSpec| {
        spec.properties()[TargetArgument::Path.name()]["description"]
            .as_str()
            .map(str::to_string)
    };
    assert_eq!(
        described(TargetSpec::NewProject).as_deref(),
        Some("Directory for the new project, outside the current one")
    );
    assert_eq!(
        described(TargetSpec::Project {
            reach: Reach::AnyProject,
            without: WithoutProject::Refused,
        })
        .as_deref(),
        Some("Project root path (the served project if omitted)")
    );
}

#[specforge_test_macros::test(
    invariant = "mcp_structured_error_responses",
    verify = "a no-project refusal names path only for an entry that takes one"
)]
fn no_project_names_path_only_where_the_entry_takes_one() {
    let adopts = |reach| NoProject {
        reach,
        serving: Serving::Adopts,
    };
    let error = no_project(adopts(Reach::AnyProject));
    assert_eq!(error.code, ErrorCode::PreconditionFailed);
    assert_eq!(error.argument.as_deref(), Some("path"));
    assert!(error.message.contains("pass {\"path\": ...}"));
    // The client can fix it: -32602 where there is no `isError`.
    assert_eq!(error.into_rpc_error().code, -32602);

    let error = no_project(adopts(Reach::Served));
    assert_eq!(error.code, ErrorCode::PreconditionFailed);
    assert_eq!(error.argument, None);
    assert!(!error.message.contains("pass {"), "{}", error.message);
    assert!(error.message.contains("specforge mcp <root>"));
    assert!(error.message.contains("call a tool that takes a `path`"));
    // No params fix it: a server-side -32603.
    assert_eq!(error.into_rpc_error().code, -32603);

    // A stateless call may serve nothing: its hint names no call that would.
    let error = no_project(NoProject {
        reach: Reach::Served,
        serving: Serving::Fixed,
    });
    assert_eq!(error.argument, None);
    assert!(
        error.message.contains("create one at its root"),
        "{}",
        error.message
    );
    assert!(!error.message.contains("call a tool"), "{}", error.message);

    // A read that named something refuses the same way, its hint the
    // entry's own.
    let target = CallTarget::EmptySession(adopts(Reach::Served));
    let refused = without_project(
        &target,
        crate::tool::entity_not_found(&specforge_graph::Graph::new(), "alpha"),
    );
    assert_eq!(refused.code, ErrorCode::PreconditionFailed);
    assert_eq!(refused.argument, None);
    assert_eq!(refused.entity_id.as_deref(), Some("alpha"));
    assert!(!refused.message.contains("pass {"), "{}", refused.message);
    let target = CallTarget::EmptySession(adopts(Reach::AnyProject));
    let refused = without_project(
        &target,
        crate::tool::entity_not_found(&specforge_graph::Graph::new(), "alpha"),
    );
    assert_eq!(refused.argument.as_deref(), Some("path"));
}

#[specforge_test_macros::test(
    invariant = "mcp_structured_error_responses",
    verify = "a path that does not exist is a file_not_found error on path"
)]
fn each_target_refusal_names_its_argument() {
    let adopts = |reach| NoProject {
        reach,
        serving: Serving::Adopts,
    };
    let path = Some("path");
    for (error, argument) in [
        (TargetError::PathNotFound(PathBuf::from("x")), path),
        (TargetError::OtherProjectRefused, path),
        (TargetError::PathRequired, path),
        (
            TargetError::InsideServed {
                dir: PathBuf::from("a"),
                served: PathBuf::from("b"),
            },
            path,
        ),
        (
            TargetError::InvalidArgument {
                argument: TargetArgument::Path,
                message: "not a string".into(),
            },
            path,
        ),
        // `no_project` names `path` only where the entry takes one.
        (TargetError::NoProject(adopts(Reach::Served)), None),
        (TargetError::NoProject(adopts(Reach::AnyProject)), path),
    ] {
        let refused = McpError::from(error.clone());
        assert_eq!(refused.argument.as_deref(), argument, "{error:?}");
    }
    assert_eq!(
        TargetError::NoProject(NoProject {
            reach: Reach::AnyProject,
            serving: Serving::Adopts
        })
        .argument(),
        None
    );
}

#[test]
fn a_target_that_needs_a_project_refuses_when_none_is_served() {
    let mut state = McpState::new();
    let revision = Revision::Negotiated(crate::lifecycle::LATEST_PROTOCOL_VERSION);
    let mut refused = |spec| resolve(&mut state, revision, spec, &serde_json::json!({}));

    assert!(matches!(
        refused(TargetSpec::SERVED_PROJECT),
        Err(TargetError::NoProject(NoProject {
            reach: Reach::Served,
            serving: Serving::Adopts
        }))
    ));
    let any = TargetSpec::Project {
        reach: Reach::AnyProject,
        without: WithoutProject::Refused,
    };
    let error = match refused(any) {
        Err(error) => error,
        Ok(_) => panic!("a project target resolved with nothing served"),
    };
    assert_eq!(
        error,
        TargetError::NoProject(NoProject {
            reach: Reach::AnyProject,
            serving: Serving::Adopts
        })
    );
    assert_eq!(McpError::from(error).argument.as_deref(), Some("path"));
    assert!(matches!(
        refused(TargetSpec::SERVED_VIEW),
        Ok(CallTarget::EmptySession(NoProject {
            reach: Reach::Served,
            serving: Serving::Adopts
        }))
    ));
    assert!(matches!(
        refused(TargetSpec::Unscoped),
        Ok(CallTarget::Unscoped)
    ));
}

#[test]
fn serving_follows_the_revision() {
    assert_eq!(
        Serving::of(Revision::Stateless("2026-07-28")),
        Serving::Fixed
    );
    for version in crate::lifecycle::PROTOCOL_VERSIONS {
        assert_eq!(
            Serving::of(Revision::Negotiated(version)),
            Serving::Adopts,
            "{version}"
        );
    }
}

#[test]
fn without_project_names_the_file_from_the_refusal() {
    let target = CallTarget::EmptySession(NoProject {
        reach: Reach::Served,
        serving: Serving::Adopts,
    });

    // The file is what the refusal says it is about, not what its
    // message happens to start with.
    let mut refusal = crate::tool::file_not_found("spec/a.spec");
    refusal.message = "the project holds no such file".to_string();
    let refused = without_project(&target, refusal);
    assert_eq!(refused.code, ErrorCode::PreconditionFailed);
    assert!(
        refused
            .message
            .contains("'spec/a.spec' is no project's file"),
        "{}",
        refused.message
    );

    // A refusal that names no file keeps its own message.
    let unnamed = McpError::new(ErrorCode::FileNotFound, "gone for another reason");
    let refused = without_project(&target, unnamed);
    assert!(
        refused.message.contains("gone for another reason"),
        "{}",
        refused.message
    );
}
