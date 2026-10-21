//! The skeleton of a new block of a kind, or of a new member: what a create of it writes in a file, with no values,
//! and a slot where each value goes (ADR 0191). Nothing is planned or judged: a skeleton holds no value (ADR 0161 D2).
//!
//! The create's own header and field writers are fed one marker word per slot (`create::header`, `create::fill`), so
//! the order, the lead, the alignment and every field's form are the create's; the markers are then taken out.

use super::block::{self, Site};
use super::{FieldInfo, Input, create, fields_of, write};
use crate::authoring::format::{Configs, Forms, Place};
use crate::view::ProjectView;
use specforge_formatter::FormatConfig;
use specforge_graph::field_form::Reading;
use specforge_protocol_types::FieldType;
use std::collections::BTreeMap;
use std::ops::Range;

/// The id and the title of a skeleton's header while its markers are in.
const ID: &str = "__id__";
const TITLE: &str = "__title__";

/// A new block or member as a create writes it. `text` reads as the block (or member) with each slot holding its
/// placeholder; `slots` are in text order, disjoint, and each `at` lies on char boundaries of `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skeleton {
    pub text: String,
    pub slots: Vec<Slot>,
}

/// Where a value goes: `text[at]` is its placeholder (empty for most fields; `id`, `Title`, `0`, `false` otherwise).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub at: Range<usize>,
    pub of: SlotOf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotOf {
    Id,
    Title,
    /// A field of the kind; for a block field the slot is its one entry's line, after its lead.
    Field(String),
    /// Where the cursor ends: an empty member line before the closing brace. A block skeleton only.
    End,
}

/// The skeletons of one file: its format configuration is looked up once (ADR 0169), as `edit::plan` looks it up.
pub struct Skeletons<'v> {
    view: &'v ProjectView<'v>,
    config: FormatConfig,
}

/// How a field's slot is found in the text the writer wrote.
enum Fill {
    /// The marker word is the slot.
    Marker,
    /// The marker and the empty text after it (a block field's one entry) are the slot.
    Entry,
    /// The whole value is the slot, holding a first value of the form (an integer or a bool takes no word).
    Literal,
}

impl<'v> Skeletons<'v> {
    /// The skeletons for a document at `place`: its project's `.specforgefmt.toml` (else the defaults), read
    /// through the view's containment, as a typed create of a block in that file reads it.
    pub fn at(view: &'v ProjectView<'v>, place: Place<'_>) -> Skeletons<'v> {
        let disk = view.containment().disk();
        let config = Configs::new(Forms::none(), &disk).layout(place);
        Skeletons { view, config }
    }

    /// A new `kind` block: its header, each field the kind requires (`FieldInfo::required`), placed, laid out and
    /// spelled as a create writes them, and an `End` slot. `None` for a kind no loaded extension declares.
    pub fn block(&self, kind: &str) -> Option<Skeleton> {
        self.view.kinds().declared(kind).ok()?;
        let site = Site {
            view: self.view,
            key: "skeleton.spec",
            id: ID,
            kind,
            config: self.config.clone(),
        };
        let required: Vec<FieldInfo> = fields_of(self.view, kind)
            .into_iter()
            .filter(|f| f.required)
            .collect();
        let mut samples = BTreeMap::new();
        let mut fills = Vec::new();
        for (n, field) in required.iter().enumerate() {
            let reading = self
                .view
                .registries()
                .field_forms()
                .reading(kind, &field.name);
            let marker = marker(n + 1);
            let (input, fill) = sample(reading, &marker)?;
            samples.insert(field.name.clone(), input);
            fills.push((field.name.clone(), marker, fill));
        }
        let header = format!("{}\n", create::header(kind, ID, TITLE));
        let text =
            create::fill(&site, block::canonical(&site, &header).ok()?, &samples, &[]).ok()?;
        let text = text.strip_suffix('\n').unwrap_or(&text);
        let mut cuts = vec![
            Cut::of(text, ID, "id", SlotOf::Id)?,
            Cut::of(text, TITLE, "Title", SlotOf::Title)?,
        ];
        for (name, marker, fill) in fills {
            cuts.push(Cut::field(text, &name, &marker, &fill)?);
        }
        let mut skeleton = apply(text, cuts);
        // The cursor's own member line before the closing brace.
        let indent = self.config.indent_str();
        skeleton.text.truncate(skeleton.text.len() - 1);
        let at = skeleton.text.len() + indent.len();
        skeleton.text.push_str(&format!("{indent}\n}}"));
        skeleton.slots.push(Slot {
            at: at..at,
            of: SlotOf::End,
        });
        Some(skeleton)
    }

    /// A new member `field` of a `kind` block, at a lead of nothing: `field <value>` in the field's form, a block
    /// field's lines one indent of the file's configuration in (the caller's client adds the line's own lead:
    /// `InsertTextMode::ADJUST_INDENTATION`). One `Field` slot. `None` for a field the kind does not declare.
    pub fn member(&self, kind: &str, field: &str) -> Option<Skeleton> {
        let reading = self.view.registries().field_forms().reading(kind, field);
        let marker = marker(1);
        let (input, fill) = sample(reading, &marker)?;
        let reading = self.view.registries().field_forms().reading(kind, field);
        let value = write::written(reading, field, &input, "", &self.config).ok()?;
        let text = format!("{field} {value}");
        Some(apply(
            &text,
            vec![Cut::field(&text, field, &marker, &fill)?],
        ))
    }
}

fn marker(n: usize) -> String {
    format!("__s{n}__")
}

/// The input the writer gets for a field read as `reading`, and how its slot is found. This is the only switch on a
/// field type in the module: it picks what to give the writer, never how to spell it.
fn sample(reading: Reading<'_>, marker: &str) -> Option<(Input, Fill)> {
    let one = || Input::One(marker.to_string());
    match reading {
        Reading::Undeclared => None,
        Reading::Member => Some((one(), Fill::Marker)),
        Reading::Declared(form) => Some(match form.field_type() {
            FieldType::Integer => (Input::One("0".into()), Fill::Literal),
            FieldType::Bool => (Input::One("false".into()), Fill::Literal),
            FieldType::ReferenceList | FieldType::StringList => {
                (Input::Many(vec![marker.to_string()]), Fill::Marker)
            }
            FieldType::Block => (
                Input::Entries(BTreeMap::from([(marker.to_string(), String::new())])),
                Fill::Entry,
            ),
            _ => (one(), Fill::Marker),
        }),
    }
}

/// `range` of the written text becomes `with`, which is a slot's placeholder.
struct Cut {
    range: Range<usize>,
    with: String,
    of: SlotOf,
}

impl Cut {
    /// The header's `marker`, replaced by `with`.
    fn of(text: &str, marker: &str, with: &str, of: SlotOf) -> Option<Cut> {
        let start = text.find(marker)?;
        Some(Cut {
            range: start..start + marker.len(),
            with: with.to_string(),
            of,
        })
    }

    /// The slot of field `name`, found by its marker, or by its key's line when the value holds no marker.
    fn field(text: &str, name: &str, marker: &str, fill: &Fill) -> Option<Cut> {
        let range = match fill {
            Fill::Marker | Fill::Entry => {
                let start = text.find(marker)?;
                let mut end = start + marker.len();
                if matches!(fill, Fill::Entry) && text[end..].starts_with(" \"\"") {
                    end += 3;
                }
                start..end
            }
            Fill::Literal => value_of(text, name)?,
        };
        // A literal value stays: it is the placeholder.
        let with = if matches!(fill, Fill::Literal) {
            text[range.clone()].to_string()
        } else {
            String::new()
        };
        Some(Cut {
            range,
            with,
            of: SlotOf::Field(name.to_string()),
        })
    }
}

/// The range of the value on the line of key `name`.
fn value_of(text: &str, name: &str) -> Option<Range<usize>> {
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let line_end = start + line.trim_end().len();
        let rest = line.trim_start().strip_prefix(name);
        if let Some(rest) = rest.filter(|r| r.starts_with(char::is_whitespace)) {
            return Some(line_end - rest.trim().len()..line_end);
        }
        start += line.len();
    }
    None
}

/// `text` with each cut replaced by its placeholder, and a slot over each.
fn apply(text: &str, mut cuts: Vec<Cut>) -> Skeleton {
    cuts.sort_by_key(|c| c.range.start);
    let (mut out, mut slots, mut from) = (String::new(), Vec::new(), 0);
    for cut in cuts {
        out.push_str(&text[from..cut.range.start]);
        let start = out.len();
        out.push_str(&cut.with);
        slots.push(Slot {
            at: start..out.len(),
            of: cut.of,
        });
        from = cut.range.end;
    }
    out.push_str(&text[from..]);
    Skeleton { text: out, slots }
}
