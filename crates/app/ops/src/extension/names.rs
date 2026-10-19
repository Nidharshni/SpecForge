//! The extensions a change names (ADR 0105): `add` and `init` read their extension strings here, once.
//! Each string is one specifier (ADR 0036): nothing splits it. A relative local path is joined onto the
//! change's base, and a local file is read within the change's containment (ADR 0104), its `..` resolved
//! as the file system resolves it. Every specifier is claimed (ADR 0098), so two that name one extension
//! are a duplicate or `extension_conflict`. A registry package is resolved by the caller's step, then
//! claimed. It then asks the change's opened lock plan for each, and names the plan's refusal
//! (ADR 0134).

mod specifier;
#[cfg(test)]
mod tests;

pub(crate) use specifier::{
    SPECIFIER_FORMS, Source, Specified, git_not_supported, named, no_specifier, parse,
};

use super::candidate::LocalFile;
use super::lock_plan::{Ask, Checked, LockPlan};
use super::{Claimed, Claims, Install};
use crate::{Containment, OpError};
use specforge_package::{PackageName, PackageRef, Version};
use specforge_wasm::WasmRuntime;
use std::path::Path;

/// Where a change reads the local files its specifiers name.
#[derive(Clone, Copy)]
pub(crate) struct Place<'r> {
    /// The project the change installs into: the lock records a local file under it relative to it.
    pub(crate) root: &'r Path,
    /// What a relative local path is joined onto.
    pub(crate) base: &'r Path,
    /// What the change may read (ADR 0104).
    pub(crate) containment: &'r Containment,
}

/// A change's extension strings, read: one specifier each, in the order given.
pub(crate) struct Names<'r> {
    wanted: Vec<Specified>,
    place: Place<'r>,
}

/// What one specifier of a change came to, once it was named and asked of the change's lock plan,
/// index for index with the strings. `R` is what the caller recorded of each install before the plan
/// took it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Answer<R> {
    /// An earlier specifier (`first`) names `name` the same way: nothing was asked (ADR 0098).
    Duplicate { name: String, first: String },
    /// A builtin, enabled under its `Ask` (its position).
    Builtin(&'static str),
    /// A local file or a registry package installed under its `Ask`: what `record` kept of the
    /// install, and, for a local file, the starter template it declares (`None` for a package).
    Installed {
        recorded: R,
        starter: Option<String>,
    },
}

/// What one specifier names, read and claimed.
enum Named<'a> {
    /// An earlier specifier names the extension the same way (or has the same source): ask nothing.
    Duplicate { name: String, first: String },
    /// A builtin, claimed first.
    Builtin(&'static str),
    /// A local file, read once and claimed first; `starter` is the starter template it declares.
    Local {
        install: Box<Install<'a>>,
        starter: Option<String>,
    },
    /// A registry package at the version the caller's step resolved, claimed first.
    Package { name: PackageName, version: Version },
}

impl<'r> Names<'r> {
    /// Read `strings`. Each is trimmed and parsed (E054 for an empty string or a package reference
    /// that is not one, R-RES-003 for a requirement that is none). The first refusal wins, named
    /// "adding '<s>': " when there are several. A relative local path is joined onto `place.base`.
    /// No file is opened and no registry is asked. An empty `strings` reads as no names: whether that
    /// is refused is the operation's call (add: E054 "no extension specifier"; init: none).
    pub(crate) fn read<S: AsRef<str>>(strings: &[S], place: Place<'r>) -> Result<Self, OpError> {
        let several = strings.len() > 1;
        let wanted = strings
            .iter()
            .map(|string| {
                let specifier = string.as_ref().trim();
                let mut source =
                    parse(specifier).map_err(|error| named(several, specifier, error))?;
                if let Source::Local(path) = &mut source
                    && path.is_relative()
                {
                    *path = place.base.join(&*path);
                }
                Ok(Specified {
                    specifier: specifier.to_string(),
                    source,
                })
            })
            .collect::<Result<_, OpError>>()?;
        Ok(Names { wanted, place })
    }

    /// The specifier at `at`, trimmed, as the caller gave it.
    pub(crate) fn specifier(&self, at: usize) -> &str {
        &self.wanted[at].specifier
    }

    /// `error`, naming the specifier at `at` when there are several (for the plan's refusals).
    fn named(&self, at: usize, error: OpError) -> OpError {
        named(self.wanted.len() > 1, self.specifier(at), error)
    }

    /// Name each specifier and ask `plan` for what it names, then check the plan once (ADR 0065,
    /// ADR 0134).
    ///
    /// Every specifier is read and claimed first ([`Self::name_each`]), then each is asked of `plan`
    /// in order, charged to `Ask(at)`, `at` its position: a builtin enabled, a local file installed,
    /// a registry package fetched from the plan's registry and installed; a duplicate asks nothing.
    /// When a specifier installs, `ask` first asks the plan ([`LockPlan::can_install`]), so a lock it
    /// cannot write refuses before any file is read or registry asked.
    ///
    /// Refusals are named by the specifier they are charged to when there are several ("adding
    /// '<s>': "): reading or `packages`, a fetch, and the plan's decisive refusal (the first ask the
    /// peer rule refused, else the first whose binary did not load). A claim conflict names both
    /// specifiers and is not prefixed. The caller states the refusal's subject.
    ///
    /// `record` is handed each install (a local file's, or the one `fetch` returned) just before the
    /// plan takes it; what it returns comes back in [`Answer::Installed`].
    pub(crate) fn ask<'a, R>(
        &self,
        mut plan: LockPlan<'a>,
        runtime: &'a dyn WasmRuntime,
        packages: &mut dyn FnMut(&str, &PackageRef) -> Result<Version, OpError>,
        record: &mut dyn FnMut(&Install<'a>) -> R,
    ) -> Result<(Checked<'a>, Vec<Answer<R>>), OpError> {
        // A change that installs refuses a lock it could not write before any file is read or registry asked (ADR 0028
        // D4): the refusal is about no specifier, so it is not named by one.
        if self.wanted.iter().any(|wanted| wanted.source.installs()) {
            plan.can_install()?;
        }
        let named = self.name_each(runtime, packages)?;
        let mut answers = Vec::with_capacity(named.len());
        for (at, named) in named.into_iter().enumerate() {
            let ask = Ask(at);
            answers.push(match named {
                Named::Duplicate { name, first } => Answer::Duplicate { name, first },
                Named::Builtin(name) => {
                    plan.enable(ask, name);
                    Answer::Builtin(name)
                }
                Named::Local { install, starter } => {
                    let recorded = record(&install);
                    plan.install(ask, *install);
                    Answer::Installed { recorded, starter }
                }
                Named::Package { name, version } => {
                    let install = plan
                        .fetch(&name, &version)
                        .map_err(|error| self.named(at, error))?;
                    let recorded = record(&install);
                    plan.install(ask, install);
                    Answer::Installed {
                        recorded,
                        starter: None,
                    }
                }
            });
        }
        let checked = plan.check().map_err(|refused| {
            let (Ask(at), error) = refused.decisive();
            self.named(at, error)
        })?;
        Ok((checked, answers))
    }

    /// What each specifier names, in order: one [`Named`] per string, index for index. `packages`
    /// resolves a registry package (`(specifier, package) -> version`). Refusals of one specifier
    /// are named among several: reading a local file (E054, E040, E028, E072, a binary declaring a
    /// builtin's name, a path outside the containment), git (E064), and whatever `packages` returns.
    /// A claim conflict is `extension_conflict`, naming both specifiers and not prefixed.
    fn name_each<'a>(
        &self,
        runtime: &'a dyn WasmRuntime,
        packages: &mut dyn FnMut(&str, &PackageRef) -> Result<Version, OpError>,
    ) -> Result<Vec<Named<'a>>, OpError> {
        let mut claims = Claims::default();
        let mut each = Vec::with_capacity(self.wanted.len());
        for (at, wanted) in self.wanted.iter().enumerate() {
            // The same source again resolves the same way: nothing is read or asked.
            if let Some(Claimed::Duplicate { name, first }) = claims.repeated(wanted) {
                each.push(Named::Duplicate { name, first });
                continue;
            }
            let claimed = match &wanted.source {
                Source::Builtin(name) => claims
                    .builtin(wanted, name)?
                    .into_named(|| Named::Builtin(name)),
                Source::Local(path) => {
                    let local = LocalFile::read(runtime, path, self.place.containment)
                        .map_err(|error| self.named(at, error))?;
                    let starter = local
                        .binary
                        .candidate()
                        .starter_template()
                        .map(str::to_string);
                    let install = Install::local(self.place.root, local)
                        .map_err(|error| self.named(at, error))?;
                    claims.local(wanted, &install)?.into_named(|| Named::Local {
                        install: Box::new(install),
                        starter,
                    })
                }
                Source::Registry(package) => {
                    let version = packages(&wanted.specifier, package)
                        .map_err(|error| self.named(at, error))?;
                    claims
                        .registry(wanted, &package.name, &version)?
                        .into_named(|| Named::Package {
                            name: package.name.clone(),
                            version,
                        })
                }
                Source::Git { url } => return Err(self.named(at, git_not_supported(url))),
            };
            each.push(claimed);
        }
        Ok(each)
    }
}

impl Claimed {
    /// `Duplicate` as it is; the first claim as what `first` makes of it.
    fn into_named<'a>(self, first: impl FnOnce() -> Named<'a>) -> Named<'a> {
        match self {
            Claimed::First => first(),
            Claimed::Duplicate { name, first } => Named::Duplicate { name, first },
        }
    }
}
