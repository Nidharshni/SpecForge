//! `specforge.add_extension`: install an extension (`specforge_ops::extension::add`).

use serde::Serialize;
use specforge_common::shape::Shape;

use crate::args::Arguments;
use crate::mutation::{
    Failed, Mutated, Mutation, MutationArgs, MutationEvent, MutationReply, NoDiagnostics, Written,
};
use crate::reply::Answer;
use crate::target::ProjectRef;

/// `specforge.add_extension`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Extension specifier
    specifier: String,
    /// Preview the install without changing any file
    dry_run: bool,
    /// Accept a registry package with no publisher signature (publisher verification skipped)
    allow_unsigned: bool,
}

impl MutationArgs for Args {
    fn previews(&self) -> bool {
        self.dry_run
    }
}

impl MutationReply for Reply {
    type After = NoDiagnostics;
}

/// `specforge.add_extension`'s reply (`McpAddExtensionResult`): a builtin
/// enabled, a package installed, an extension already there, or a preview.
#[derive(Debug, Serialize, Shape)]
#[serde(untagged)]
pub enum Reply {
    Builtin(Builtin),
    Package(Package),
    AlreadyPresent(AlreadyPresent),
    Planned(Planned),
}

/// The one spelling of a builtin's `source`.
#[derive(Debug, Serialize, Shape)]
pub enum BuiltinSource {
    #[serde(rename = "builtin")]
    Builtin,
}

/// A builtin enabled.
#[derive(Debug, Serialize, Shape)]
pub struct Builtin {
    extension: String,
    installed: bool,
    source: BuiltinSource,
    changed: bool,
    /// The required builtin peers enabled first.
    peers_enabled: Vec<String>,
    note: String,
}

/// A package installed and locked.
#[derive(Debug, Serialize, Shape)]
pub struct Package {
    extension: String,
    installed: bool,
    version: String,
    sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    key_id: Option<String>,
    /// How the publisher was accepted: `unsigned`, `pinned`, `pinned_now` or `repinned`; absent for
    /// a local file.
    #[serde(skip_serializing_if = "Option::is_none")]
    publisher: Option<String>,
    source: String,
    note: String,
}

/// Already installed and enabled: nothing changed.
#[derive(Debug, Serialize, Shape)]
pub struct AlreadyPresent {
    extension: String,
    installed: bool,
    already_present: bool,
    version: String,
    message: String,
}

/// A preview: what would be installed or enabled.
#[derive(Debug, Serialize, Shape)]
pub struct Planned {
    extension: String,
    installed: bool,
    dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    source: String,
}

/// `specforge.add_extension`: the shared add, its reply, the files it
/// wrote and `extension_added` (for an extension already there too,
/// `wasDuplicate`).
pub(crate) fn call(project: &ProjectRef<'_>, args: Args) -> Mutation<Reply> {
    use specforge_ops::extension::{self, AddOutcome, AddRequest, Origin, Trust};

    let allow_unsigned = args.allow_unsigned;

    // The project the call installs into: the served one, or the one
    // `path` names.
    let root = project.root.to_path_buf();
    let served = project.is_served();
    // Once it is enabled, the server serves it (the next request brings the
    // project up to date); another project only has it on disk.
    let note = if served {
        "the server serves it from the next call on"
    } else {
        "installed in the project the path names; the server keeps serving its own"
    };
    let source = extension::parse(&args.specifier)?;

    let registry = specforge_ops_registry::ConfiguredRegistry::for_project(&root, "add_extension");
    // The shared operation `specforge add` runs. An agent can't be asked,
    // so a publisher key change is refused rather than re-pinned.
    let request = AddRequest {
        root: &root,
        source,
        allow_unsigned,
        trust: Trust::Refuse,
        dry_run: args.dry_run,
    };
    let added = extension::add(&request, &registry, project.runtime.as_ref());
    // What reading the registry configuration reported (E067, W140, I003), once the add asked a
    // registry, as `specforge add` shows it.
    let reported = registry.reported();
    // An install that failed after placing its module reports it.
    let added = match added {
        Ok(added) => added,
        Err(error) => return Err(Failed::from(error).with_diagnostics(reported)),
    };
    let source_of = Origin::source;
    let (reply, was_duplicate) = match added.outcome {
        AddOutcome::Builtin {
            name,
            changed,
            peers_enabled,
        } => (
            Reply::Builtin(Builtin {
                extension: name.to_string(),
                installed: changed,
                source: BuiltinSource::Builtin,
                changed,
                peers_enabled: peers_enabled.into_iter().map(String::from).collect(),
                note: note.to_string(),
            }),
            !changed,
        ),
        AddOutcome::Installed {
            name,
            version,
            sha256,
            publisher,
            origin,
        } => (
            Reply::Package(Package {
                extension: name,
                installed: true,
                version,
                sha256,
                key_id: publisher
                    .as_ref()
                    .and_then(|p| p.key_id())
                    .map(str::to_string),
                publisher: publisher.as_ref().map(|p| p.as_str().to_string()),
                source: source_of(&origin),
                note: note.to_string(),
            }),
            false,
        ),
        // Already installed and enabled: an info response, nothing changed.
        AddOutcome::AlreadyPresent { name, version } => (
            Reply::AlreadyPresent(AlreadyPresent {
                message: format!(
                    "{name} {version} is already installed; specforge.json is unchanged"
                ),
                extension: name,
                installed: false,
                already_present: true,
                version,
            }),
            true,
        ),
        AddOutcome::Planned {
            name,
            version,
            origin,
        } => {
            let plan = Reply::Planned(Planned {
                extension: name,
                installed: false,
                dry_run: true,
                version,
                source: source_of(&origin),
            });
            return Ok(Mutated::new(
                Answer::new(plan).with_diagnostics(reported),
                Written::nothing(),
            ));
        }
    };
    // A dry run of an extension already present answers as the add would (`AlreadyPresent`, or a
    // builtin left as it was): a preview, which writes and records nothing.
    if dry_run {
        return Ok(Mutated::preview(
            Answer::new(reply).with_diagnostics(reported),
        ));
    }
    let event = MutationEvent::ExtensionAdded {
        specifier: args.specifier,
        total_extensions: added.extensions_enabled,
        was_duplicate,
    };
    Ok(Mutated::new(
        Answer::new(reply).with_diagnostics(reported),
        Written::files(added.writes).with_event(event),
    ))
}
