//! The project's extensions, one row per name: what the extensions listing,
//! doctor and remove read to know which extensions a project has, where each
//! comes from and whether it loaded (ADR 0028, "The project's extensions are
//! one inventory").

use super::{Origin, builtin_name};
use crate::view::ProjectView;
use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_installed::{EnabledExtension, LoadFailure, LockFileEntry};
use specforge_protocol_types::ExtensionDeclaration;
use std::collections::BTreeSet;

/// Whether an extension of the project is part of the compiled project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Enabled by a `specforge.json` entry, and loaded.
    Loaded,
    /// Enabled, but not loaded: not installed, or it failed to load
    /// ([`ProjectExtension::failures`] says why).
    NotLoaded,
    /// Locked (or loaded), but no entry enables it.
    NotConfigured,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Loaded => "loaded",
            Status::NotLoaded => "not_loaded",
            Status::NotConfigured => "not_configured",
        }
    }
}

/// Every extension a project has: enabled by a `specforge.json` entry,
/// pinned in the `specforge.lock` the compile read, or loaded by the
/// compile; one row per name, sorted by name. Without a root there is no
/// lock ([`ProjectView::lock`]), so the rows are what the view enabled and
/// loaded.
#[derive(Debug, Clone)]
pub struct Inventory<'a> {
    /// What each `specforge.json` entry enabled, in its order (the view's
    /// `env().enabled()`): an ambiguous selection names them in this order.
    entries: &'a [EnabledExtension],
    rows: Vec<ProjectExtension<'a>>,
}

/// One extension of the project: everything the compile knows of it under
/// its name.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectExtension<'a> {
    /// A named entry's name; the name a `.wasm` file entry's component
    /// declared, or (when it did not load) the name it writes before `=`,
    /// else its path; a lock entry's name; a loaded declaration's name.
    pub name: &'a str,
    /// Where it comes from, by the rule the extension load follows: a
    /// `.wasm` file when only file entries enable it; else a builtin for a
    /// builtin's name; else the lock entry's source; else `Unknown`.
    pub origin: Origin,
    pub status: Status,
    /// The `specforge.json` entries that enable it, in config order. Empty:
    /// no entry enables it.
    pub entries: Vec<&'a EnabledExtension>,
    /// Its declaration, when the compile loaded it.
    pub declaration: Option<&'a ExtensionDeclaration>,
    /// Its `specforge.lock` entry, when the lock the compile read pins this
    /// name.
    pub lock: Option<&'a LockFileEntry>,
}

/// What a `remove` argument names in the project ([`Inventory::select`]).
#[derive(Debug, PartialEq)]
pub enum Selection<'i, 'a> {
    /// One `.wasm` file entry, named by its text, its path or the name it
    /// enabled, and no named entry of that name: the entry, and the row of
    /// the extension it enabled.
    File {
        entry: &'a EnabledExtension,
        extension: &'i ProjectExtension<'a>,
    },
    /// The extension of that name; no `.wasm` file entry names it.
    Named(&'i ProjectExtension<'a>),
    /// More than one entry, at least one a `.wasm` file entry: those
    /// entries, in `specforge.json` order.
    Ambiguous(Vec<&'a EnabledExtension>),
    /// No entry, lock entry or loaded extension has that name, and no file
    /// entry has that text or path.
    Nothing,
}

/// The project's extensions as the compile behind `view` knows them: what
/// each `specforge.json` entry enabled, the lock the compile read (none
/// without a root) and the declarations it loaded, never read again.
pub fn inventory<'a>(view: &ProjectView<'a>) -> Inventory<'a> {
    let entries: &'a [EnabledExtension] = view.env().enabled();
    let declarations: &'a [ExtensionDeclaration] = view.registries().declarations();
    let locked: &'a [LockFileEntry] = view.lock().entries();
    let names: BTreeSet<&'a str> = entries
        .iter()
        .map(|e| e.name.as_str())
        .chain(locked.iter().map(|e| e.name.as_str()))
        .chain(declarations.iter().map(|d| d.name()))
        .collect();
    let rows = names
        .into_iter()
        .map(|name| {
            let enabling: Vec<&'a EnabledExtension> =
                entries.iter().filter(|e| e.name == name).collect();
            let declaration = declarations.iter().find(|d| d.name() == name);
            let lock = locked.iter().find(|e| e.name.as_str() == name);
            ProjectExtension {
                origin: origin(name, &enabling, lock),
                status: match (enabling.is_empty(), declaration) {
                    (false, Some(_)) => Status::Loaded,
                    (false, None) => Status::NotLoaded,
                    (true, _) => Status::NotConfigured,
                },
                name,
                entries: enabling,
                declaration,
                lock,
            }
        })
        .collect();
    Inventory { entries, rows }
}

/// Where `name` comes from, in the order the extension load tries
/// (`Installed::load`): named entries load before `.wasm` file entries,
/// and a named entry is a builtin before it is a lock entry. So a `.wasm`
/// file is the origin only when no named entry enables the name.
fn origin(name: &str, entries: &[&EnabledExtension], lock: Option<&LockFileEntry>) -> Origin {
    let named = entries.iter().any(|e| e.file.is_none());
    if !named && let Some(path) = entries.iter().find_map(|e| e.file.clone()) {
        return Origin::File { path };
    }
    if builtin_name(name).is_some() {
        return Origin::Builtin;
    }
    match lock {
        Some(entry) => Origin::Installed {
            source: entry.source.clone(),
        },
        None => Origin::Unknown,
    }
}

impl<'a> ProjectExtension<'a> {
    /// Some entry enables it.
    pub fn enabled(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Its version: the loaded declaration's; else, for an installed
    /// extension, the locked one; else, for an extension of unknown origin,
    /// the version a legacy `name@version` entry writes; else none (a
    /// builtin or a `.wasm` file that did not load).
    pub fn version(&self) -> Option<&'a str> {
        if let Some(declaration) = self.declaration {
            return Some(declaration.version());
        }
        match &self.origin {
            Origin::Installed { .. } => self.lock.map(|entry| entry.version.as_str()),
            // `@acme/x@1.2.0`, as older `specforge add`s wrote it.
            Origin::Unknown => self.entries.iter().find_map(|e| {
                e.entry
                    .strip_prefix(self.name)
                    .and_then(|rest| rest.strip_prefix('@'))
            }),
            Origin::Builtin | Origin::File { .. } => None,
        }
    }

    /// Why its entries did not load, in config order (an entry that loaded,
    /// or that another entry made redundant, has none).
    pub fn failures(&self) -> impl Iterator<Item = &'a LoadFailure> + '_ {
        self.entries.iter().filter_map(|e| e.failure.as_ref())
    }
}

impl<'a> Inventory<'a> {
    /// Every row, sorted by name.
    pub fn rows(&self) -> &[ProjectExtension<'a>] {
        &self.rows
    }

    /// The row named `name`.
    pub fn get(&self, name: &str) -> Option<&ProjectExtension<'a>> {
        self.rows.iter().find(|row| row.name == name)
    }

    /// What `argument` names (a `remove` argument): a `.wasm` file entry by
    /// its text as `specforge.json` writes it, its path or the name it
    /// enabled; else the extension of that name.
    pub fn select(&self, argument: &str) -> Selection<'_, 'a> {
        let files: Vec<&'a EnabledExtension> = self
            .entries
            .iter()
            .filter(|e| {
                e.file.as_deref().is_some_and(|path| {
                    e.entry == argument || path == argument || e.name == argument
                })
            })
            .collect();
        let named = |e: &EnabledExtension| e.file.is_none() && e.name == argument;
        match (files.as_slice(), self.entries.iter().any(named)) {
            ([], _) => self
                .get(argument)
                .map_or(Selection::Nothing, Selection::Named),
            ([entry], false) => Selection::File {
                entry,
                extension: self
                    .get(&entry.name)
                    .expect("every entry's name has its row"),
            },
            _ => Selection::Ambiguous(
                self.entries
                    .iter()
                    .filter(|e| files.contains(e) || named(e))
                    .collect(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::testing::Fixture;
    use specforge_common::{Diagnostic, codes};
    use specforge_installed::{LoadProblem, LockSource};
    use specforge_test_macros::test as specforge_test;

    /// What the load records for an entry it could not load.
    fn failing(entry: &str, problem: LoadProblem) -> EnabledExtension {
        let entry = EnabledExtension::unloaded(entry);
        EnabledExtension {
            failure: Some(LoadFailure {
                diagnostic: Diagnostic::new(codes::E028, format!("{} did not load", entry.entry)),
                problem,
            }),
            ..entry
        }
    }

    /// One row, as a table compares it.
    #[derive(Debug, PartialEq)]
    struct Row {
        name: String,
        origin: Origin,
        status: Status,
        version: Option<String>,
        entries: usize,
        declared: bool,
        locked: bool,
        problems: Vec<LoadProblem>,
    }

    fn table(view: &ProjectView<'_>) -> Vec<Row> {
        inventory(view)
            .rows()
            .iter()
            .map(|r| Row {
                name: r.name.to_string(),
                origin: r.origin.clone(),
                status: r.status,
                version: r.version().map(str::to_string),
                entries: r.entries.len(),
                declared: r.declaration.is_some(),
                locked: r.lock.is_some(),
                problems: r.failures().map(|f| f.problem.clone()).collect(),
            })
            .collect()
    }

    fn row(
        name: &str,
        origin: Origin,
        status: Status,
        version: Option<&str>,
        (entries, declared, locked): (usize, bool, bool),
        problems: Vec<LoadProblem>,
    ) -> Row {
        Row {
            name: name.into(),
            origin,
            status,
            version: version.map(str::to_string),
            entries,
            declared,
            locked,
            problems,
        }
    }

    #[specforge_test(
        behavior = "management_operations_over_the_project_view",
        verify = "list, doctor and remove read one inventory of the project's extensions"
    )]
    fn one_row_per_name_with_its_origin_status_version_and_facts() {
        let changed = LoadProblem::Changed {
            locked: "a".into(),
            actual: "b".into(),
        };
        let fixture = Fixture::new()
            .enabled(vec![
                EnabledExtension::unloaded("@specforge/product"),
                EnabledExtension::unloaded("@acme/locked"),
                EnabledExtension {
                    name: "@sdk/greet".into(),
                    ..EnabledExtension::unloaded("greet.wasm")
                },
                failing("@acme/missing@1.2.0", LoadProblem::NotInstalled),
                failing("@acme/changed", changed.clone()),
            ])
            .declarations(vec![
                Fixture::declaration("@specforge/product", "1.0.0"),
                Fixture::declaration("@acme/locked", "2.0.0"),
                Fixture::declaration("@sdk/greet", "0.1.0"),
            ])
            .lock(&[
                ("@acme/locked", "2.0.0", "registry"),
                ("@acme/stale", "3.0.0", "local:/x/stale.wasm"),
                ("@acme/changed", "4.0.0", "registry"),
            ]);
        let registry = Origin::Installed {
            source: LockSource::Registry,
        };

        assert_eq!(
            table(&fixture.view()),
            [
                row(
                    "@acme/changed",
                    registry.clone(),
                    Status::NotLoaded,
                    Some("4.0.0"),
                    (1, false, true),
                    vec![changed]
                ),
                row(
                    "@acme/locked",
                    registry,
                    Status::Loaded,
                    Some("2.0.0"),
                    (1, true, true),
                    vec![]
                ),
                row(
                    "@acme/missing",
                    Origin::Unknown,
                    Status::NotLoaded,
                    Some("1.2.0"),
                    (1, false, false),
                    vec![LoadProblem::NotInstalled]
                ),
                row(
                    "@acme/stale",
                    Origin::Installed {
                        source: LockSource::Local("/x/stale.wasm".into())
                    },
                    Status::NotConfigured,
                    Some("3.0.0"),
                    (0, false, true),
                    vec![]
                ),
                row(
                    "@sdk/greet",
                    Origin::File {
                        path: "greet.wasm".into()
                    },
                    Status::Loaded,
                    Some("0.1.0"),
                    (1, true, false),
                    vec![]
                ),
                row(
                    "@specforge/product",
                    Origin::Builtin,
                    Status::Loaded,
                    Some("1.0.0"),
                    (1, true, false),
                    vec![]
                ),
            ]
        );

        // Without a root there is no lock: the rows are what was enabled
        // and loaded.
        let rootless = table(&fixture.rootless_view());
        let names: Vec<&str> = rootless.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "@acme/changed",
                "@acme/locked",
                "@acme/missing",
                "@sdk/greet",
                "@specforge/product"
            ]
        );
        assert!(
            rootless
                .iter()
                .filter(|r| r.name == "@acme/locked" || r.name == "@acme/changed")
                .all(|r| r.origin == Origin::Unknown && !r.locked)
        );
    }

    #[specforge_test(
        behavior = "list_installed_extensions",
        verify = "an extension's source is where the extension load takes it from, whatever order its entries are in"
    )]
    fn an_extensions_source_is_where_the_load_takes_it() {
        // A builtin's name in the lock is a stray entry: the builtin loads.
        let fixture = Fixture::new()
            .config(&["@specforge/product"])
            .declarations(vec![Fixture::declaration("@specforge/product", "1.0.0")])
            .lock(&[("@specforge/product", "9.9.9", "registry")]);
        let rows = table(&fixture.view());
        assert_eq!(rows[0].origin, Origin::Builtin);
        assert_eq!(rows[0].version.as_deref(), Some("1.0.0"));

        // A named and a file entry of one name, in both orders: the named
        // entry's source.
        let refused = failing(
            "@sdk/greet=greet.wasm",
            LoadProblem::AlreadyLoaded {
                declared: "@sdk/greet".into(),
            },
        );
        let named = EnabledExtension::unloaded("@sdk/greet");
        for enabled in [
            vec![refused.clone(), named.clone()],
            vec![named.clone(), refused.clone()],
        ] {
            let fixture = Fixture::new()
                .enabled(enabled)
                .declarations(vec![Fixture::declaration("@sdk/greet", "0.1.0")])
                .lock(&[("@sdk/greet", "0.1.0", "registry")]);
            assert_eq!(
                table(&fixture.view())[0].origin,
                Origin::Installed {
                    source: LockSource::Registry
                }
            );
        }
    }

    #[test]
    fn a_file_entry_that_did_not_load_has_no_version() {
        let fixture = Fixture::new()
            .enabled(vec![failing(
                "@test/probe=nope.wasm",
                LoadProblem::FileMissing {
                    path: "nope.wasm".into(),
                },
            )])
            .lock(&[("@test/probe", "0.1.0", "local:/blobs/probe.wasm")]);

        let probe = &table(&fixture.view())[0];

        assert_eq!(
            probe.origin,
            Origin::File {
                path: "nope.wasm".into()
            }
        );
        assert_eq!(probe.status, Status::NotLoaded);
        assert_eq!(probe.version, None);
        assert!(probe.locked);
    }

    #[test]
    fn an_argument_selects_a_file_entry_a_name_or_both() {
        let fixture = Fixture::new().enabled(vec![
            EnabledExtension::unloaded("@sdk/greet=greet.wasm"),
            EnabledExtension::unloaded("@sdk/twice=twice.wasm"),
            EnabledExtension::unloaded("@sdk/twice=twice.wasm"),
            EnabledExtension::unloaded("@sdk/both"),
            EnabledExtension::unloaded("@sdk/both=both.wasm"),
            EnabledExtension::unloaded("@acme/named"),
            EnabledExtension::unloaded("Bad Name"),
        ]);
        let view = fixture.view();
        let inventory = inventory(&view);

        for argument in ["@sdk/greet=greet.wasm", "greet.wasm", "@sdk/greet"] {
            let Selection::File { entry, extension } = inventory.select(argument) else {
                panic!("{argument}");
            };
            assert_eq!(entry.entry, "@sdk/greet=greet.wasm", "{argument}");
            assert_eq!(extension.name, "@sdk/greet", "{argument}");
        }
        assert!(matches!(
            inventory.select("@acme/named"),
            Selection::Named(row) if row.name == "@acme/named"
        ));
        let Selection::Ambiguous(entries) = inventory.select("@sdk/both") else {
            panic!("both");
        };
        let texts: Vec<&str> = entries.iter().map(|e| e.entry.as_str()).collect();
        assert_eq!(texts, ["@sdk/both", "@sdk/both=both.wasm"]);
        assert!(matches!(
            inventory.select("@sdk/twice"),
            Selection::Ambiguous(entries) if entries.len() == 2
        ));
        assert_eq!(inventory.select("@nobody/x"), Selection::Nothing);
        assert!(matches!(
            inventory.select("Bad Name"),
            Selection::Named(row) if row.name == "Bad Name"
        ));
    }
}
