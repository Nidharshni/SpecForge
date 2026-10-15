//! The commands' routing table: the one place that decides which command a
//! name runs (ADR 0097).

use specforge_common::{Diagnostic, codes};
use specforge_protocol_types::command_args::option_name;
use specforge_protocol_types::{CommandDescriptor, ExtensionDeclaration};

/// One extension command and the name it is routed under:
/// `specforge <short> <cli_name>`, MCP `specforge.<short>.<id>` (ADR 0097).
#[derive(Debug, Clone, PartialEq)]
pub struct CommandRoute {
    extension: String,
    short: String,
    cli_name: String,
    declaration: CommandDescriptor,
}

impl CommandRoute {
    fn new(declaration: &ExtensionDeclaration, command: &CommandDescriptor) -> Self {
        Self {
            extension: declaration.name().to_string(),
            short: declaration.short().into_owned(),
            cli_name: option_name(&command.id),
            declaration: command.clone(),
        }
    }

    /// The extension that declares it.
    pub fn extension(&self) -> &str {
        &self.extension
    }

    /// Its extension's short name.
    pub fn short(&self) -> &str {
        &self.short
    }

    /// Its name on the command line: its id, `_` spelled `-`.
    pub fn cli_name(&self) -> &str {
        &self.cli_name
    }

    /// The command as declared.
    pub fn declaration(&self) -> &CommandDescriptor {
        &self.declaration
    }

    fn name(&self) -> (&str, &str) {
        (&self.short, &self.cli_name)
    }
}

/// A command routed under the name of a command earlier in load (then
/// declaration) order: it runs on no surface.
#[derive(Debug, Clone, PartialEq)]
pub struct ShadowedRoute {
    route: CommandRoute,
    by: usize,
}

impl ShadowedRoute {
    /// The command that is not routed.
    pub fn route(&self) -> &CommandRoute {
        &self.route
    }

    /// The index, in [`CommandRoutes::routed`], of the command that has its
    /// name.
    pub fn by(&self) -> usize {
        self.by
    }
}

/// The project's commands, routed by (short name, CLI name): the one place
/// that decides which command a name runs (ADR 0097).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommandRoutes {
    routed: Vec<CommandRoute>,
    shadowed: Vec<ShadowedRoute>,
}

impl CommandRoutes {
    /// The commands that run, in load then declaration order.
    pub fn routed(&self) -> &[CommandRoute] {
        &self.routed
    }

    /// The commands that run on no surface.
    pub fn shadowed(&self) -> &[ShadowedRoute] {
        &self.shadowed
    }

    /// The command routed under `shadowed`'s name.
    pub fn winner(&self, shadowed: &ShadowedRoute) -> &CommandRoute {
        &self.routed[shadowed.by]
    }

    /// Route `command` of `declaration`; `Some(E039)` when an earlier
    /// command has its name.
    pub(crate) fn add(
        &mut self,
        declaration: &ExtensionDeclaration,
        command: &CommandDescriptor,
    ) -> Option<Diagnostic> {
        let route = CommandRoute::new(declaration, command);
        let Some(by) = self.routed.iter().position(|r| r.name() == route.name()) else {
            self.routed.push(route);
            return None;
        };
        let winner = &self.routed[by];
        let message = if winner.extension == route.extension {
            format!(
                "command '{}' of '{}' is not routed: '{}' routes `specforge {} {}` first, as '{}'",
                route.declaration.id,
                route.extension,
                winner.extension,
                route.short,
                route.cli_name,
                winner.declaration.id
            )
        } else {
            format!(
                "command '{}' of '{}' is not routed: '{}' routes `specforge {} {}` first",
                route.declaration.id,
                route.extension,
                winner.extension,
                route.short,
                route.cli_name
            )
        };
        self.shadowed.push(ShadowedRoute { route, by });
        Some(
            Diagnostic::new(codes::E039, message).with_suggestion(
                "rename one of the commands, or give one extension another short name (ext_short; \
             the SDK's #[extension(short = \"...\")])"
                    .to_string(),
            ),
        )
    }
}
