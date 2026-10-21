//! The builtins and companions a change enables because what it adds requires them (ADR 0041 D4).

use super::super::peer_set::{Modules, ProjectPeers};
use super::{Appended, Ask, Changing, LockPlan};
use crate::OpError;
use specforge_common::ExtensionEntry;

impl<'a> LockPlan<'a> {
    /// Enable what the change requires (a verb that `enables`; `update`'s plan enables nothing):
    /// each required peer of an extension the change touches that is a builtin no entry enables,
    /// and theirs, each appended just before the entry that first requires it (dependencies
    /// first). E028 when a builtin's declaration can't be read. Idempotent; [`Self::check`] runs
    /// it first.
    ///
    /// A cycle among required builtins ends it too: its builtins are enabled once, and the cycle's
    /// E027 stays the compile's (ADR 0041 D4).
    pub(super) fn close(&mut self) -> Result<(), OpError> {
        if !self.verb.enables() {
            return Ok(());
        }
        loop {
            let set = ProjectPeers::of(
                &self.view()?,
                &self.entries_after(),
                &mut self.declarations,
                Modules::Locked,
            )?;
            let changing = Changing::of(&self.staged, &self.appended);
            let mut wanted: Vec<(String, &'static str)> = Vec::new();
            for name in &changing.names {
                let Some(member) = set.member(name) else {
                    continue;
                };
                for peer in member.peers.iter().filter(|peer| !peer.optional) {
                    if let Some(builtin) = super::super::builtin_name(&peer.name)
                        && set.member(builtin).is_none()
                        && !wanted.iter().any(|(_, wanted)| *wanted == builtin)
                    {
                        wanted.push((name.clone(), builtin));
                    }
                }
            }
            if wanted.is_empty() {
                if self.accompany_enabled() {
                    continue;
                }
                self.dependencies_first(&set);
                return Ok(());
            }
            for (dependent, builtin) in wanted {
                let (ask, root) = match self
                    .appended
                    .iter()
                    .find(|appended| appended.name == dependent)
                {
                    Some(appended) => (
                        appended.ask,
                        appended
                            .required_by
                            .clone()
                            .unwrap_or_else(|| dependent.clone()),
                    ),
                    None => (
                        self.staged
                            .iter()
                            .find(|staged| staged.name.as_str() == dependent)
                            .map_or(Ask(0), |staged| staged.ask),
                        dependent.clone(),
                    ),
                };
                self.appended.push(Appended {
                    name: builtin.to_string(),
                    ask,
                    required_by: Some(root),
                });
            }
        }
    }

    /// Append each companion whose trigger the change leaves enabled and that no entry enables:
    /// whether it appended any.
    fn accompany_enabled(&mut self) -> bool {
        let mut added = false;
        for (trigger, companion) in self.companions.clone() {
            let after = self.entries_after();
            let enabled = |name: &str| {
                after
                    .iter()
                    .any(|entry| ExtensionEntry::parse(entry).names(name))
            };
            if enabled(trigger) && !enabled(companion) {
                let ask = self
                    .appended
                    .iter()
                    .find(|appended| appended.name == trigger)
                    .map(|appended| appended.ask)
                    .or_else(|| {
                        self.staged
                            .iter()
                            .find(|staged| staged.name.as_str() == trigger)
                            .map(|staged| staged.ask)
                    })
                    .unwrap_or(Ask(0));
                self.appended.push(Appended {
                    name: companion.to_string(),
                    ask,
                    required_by: None,
                });
                added = true;
            }
        }
        added
    }

    /// The appended entries, each after the appended builtins it requires (its non-optional
    /// peers): a depth-first order, otherwise the order they were enabled in. A cycle puts each of
    /// its entries once.
    fn dependencies_first(&mut self, set: &ProjectPeers) {
        let names: Vec<String> = self
            .appended
            .iter()
            .map(|appended| appended.name.clone())
            .collect();
        let mut order: Vec<usize> = Vec::with_capacity(names.len());
        let mut visited = vec![false; names.len()];
        fn visit(
            at: usize,
            names: &[String],
            set: &ProjectPeers,
            visited: &mut [bool],
            order: &mut Vec<usize>,
        ) {
            if std::mem::replace(&mut visited[at], true) {
                return;
            }
            if let Some(member) = set.member(&names[at]) {
                for peer in member.peers.iter().filter(|peer| !peer.optional) {
                    if super::super::builtin_name(&peer.name).is_none() {
                        continue;
                    }
                    if let Some(next) = names.iter().position(|name| *name == peer.name) {
                        visit(next, names, set, visited, order);
                    }
                }
            }
            order.push(at);
        }
        for at in 0..names.len() {
            visit(at, &names, set, &mut visited, &mut order);
        }
        let mut taken: Vec<Option<Appended>> = std::mem::take(&mut self.appended)
            .into_iter()
            .map(Some)
            .collect();
        self.appended = order
            .into_iter()
            .map(|at| taken[at].take().expect("each entry is placed once"))
            .collect();
    }
}
