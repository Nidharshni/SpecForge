use crate::target::Unscoped;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_diagnostics::{CodeEntry, docs_href, lookup, retired};

use crate::args::Arguments;
use crate::reply::Answered;
use crate::tool::McpError;

/// `specforge.explain`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// A diagnostic code, any case
    code: String,
}

/// `specforge.explain`'s reply: a catalogued code's
/// entry, or a retired code and the entry that replaced it.
#[derive(Debug, Serialize, Shape)]
#[serde(untagged)]
pub enum Reply {
    Entry(Explained),
    Retired(Retired),
}

/// A code the catalogue has.
#[derive(Debug, Serialize, Shape)]
pub struct Explained {
    #[serde(flatten)]
    entry: Entry,
    retired: bool,
}

/// A code the catalogue retired.
#[derive(Debug, Serialize, Shape)]
pub struct Retired {
    code: String,
    retired: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    replaced_by: Option<Entry>,
}

/// One catalogue entry.
#[derive(Debug, Serialize, Shape)]
pub struct Entry {
    code: String,
    title: String,
    owner: String,
    level: String,
    explanation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    docs: Option<String>,
}

impl Entry {
    fn of(entry: &CodeEntry) -> Self {
        Entry {
            code: entry.code.to_string(),
            title: entry.title.to_string(),
            owner: entry.owner.to_string(),
            level: entry.level.describe().to_string(),
            explanation: entry.explanation.to_string(),
            docs: docs_href(entry.code),
        }
    }
}

/// `specforge.explain`: what `specforge explain <code>` prints, as data.
/// A retired code names the entry that replaced it, if any; a code the
/// catalogue doesn't have is invalid input.
pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.explain";
    const DESCRIPTION: &'static str = "Explain a diagnostic code: its title, owner, level, what triggers it and how to fix it, and its docs link";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Core,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpExplainResult");
    type Args = Args;
    type Input = Unscoped;
    type Returns = Answered<Reply>;

    fn call(_: (), args: Args) -> Answered<Reply> {
        let code = args.code.trim();
        if let Some(replacement) = retired(code) {
            return Ok(Reply::Retired(Retired {
                code: code.to_uppercase(),
                retired: true,
                replaced_by: replacement.and_then(lookup).map(Entry::of),
            })
            .into());
        }
        match lookup(code) {
            Some(entry) => Ok(Reply::Entry(Explained {
                entry: Entry::of(entry),
                retired: false,
            })
            .into()),
            None => Err(Box::new(McpError::invalid_input(
                "code",
                format!(
                    "Unknown diagnostic code: {code}. Catalogued codes are E###, W###, I###, A###, \
                 R### and R-<AREA>-###; E900-E998, W900-W998 and I900-I998 belong to \
                 third-party extensions"
                ),
            ))),
        }
    }
}
