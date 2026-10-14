//! `@test/ext`: the extension MCP's tests serve, declared with the SDK
//! builders an extension author uses.

use std::sync::Arc;

use specforge_extension_sdk::ExtensionDeclaration;
use specforge_extension_sdk::prelude::*;
use specforge_wasm::testing::InProcessRuntime;

/// The name the test extension is declared and enabled under.
pub const EXT: &str = "@test/ext";

/// What a test adds with the SDK builders beyond the kinds.
type Declare = Arc<dyn Fn(&mut ContributionsBuilder) + Send + Sync>;

/// One field a kind of the test extension declares.
#[derive(Clone, Debug)]
struct Field {
    name: String,
    field_type: FieldType,
    /// The kind a reference list targets, its edges labelled by the field.
    target: Option<String>,
    normative: bool,
    headline: bool,
}

impl Field {
    fn string(name: &str) -> Self {
        Field {
            name: name.to_string(),
            field_type: FieldType::String,
            target: None,
            normative: false,
            headline: false,
        }
    }

    fn reference_list(name: &str, target: &str) -> Self {
        Field {
            name: name.to_string(),
            field_type: FieldType::ReferenceList,
            target: Some(target.to_string()),
            normative: false,
            headline: false,
        }
    }
}

/// One kind the test extension declares.
#[derive(Clone, Debug)]
struct Kind {
    name: String,
    /// Testable kinds accept `verify` obligations.
    testable: bool,
    /// In declaration order.
    fields: Vec<Field>,
}

impl Kind {
    /// The field `name`, declared as `field` makes it when the kind has none
    /// of that name yet.
    fn field(&mut self, name: &str, field: impl FnOnce() -> Field) -> &mut Field {
        let at = match self.fields.iter().position(|f| f.name == name) {
            Some(at) => at,
            None => {
                self.fields.push(field());
                self.fields.len() - 1
            }
        };
        &mut self.fields[at]
    }
}

/// An extension declared with the SDK builders an extension author uses,
/// loaded by the host as it loads any extension: its declaration crosses
/// the protocol (`ExtensionDeclaration`) and the registry build turns it
/// into kinds, fields, edges and rules. Tests never see a registry entry,
/// so a change to the registry's types changes no test.
#[derive(Clone)]
pub struct TestExtension {
    name: String,
    /// In declaration order.
    kinds: Vec<Kind>,
    /// Kinds W004 targets (`no_verify_statements` on `verify`).
    obligated: Vec<String>,
    /// What a `check`-phase pass `report` answers on every compile.
    reported: Vec<PassDiagnostic>,
    declares: Vec<Declare>,
}

impl Default for TestExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl TestExtension {
    /// `@test/ext`, declaring nothing yet.
    pub fn new() -> Self {
        Self::named(EXT)
    }

    /// An extension named `name` (a second one: a provider, a conflicting
    /// kind), declaring nothing yet.
    pub fn named(name: &str) -> Self {
        TestExtension {
            name: name.to_string(),
            kinds: Vec::new(),
            obligated: Vec::new(),
            reported: Vec::new(),
            declares: Vec::new(),
        }
    }

    /// The kinds MCP tests share, as `@specforge/software` shapes them:
    /// `behavior` (testable, accepts `verify`; `contract` string field),
    /// `invariant` (testable, accepts `verify`; `guarantee` string field)
    /// and `feature` (not testable; `behaviors` → behavior and `invariants`
    /// → invariant reference lists, each edge labelled by its field).
    /// Nothing is obligated and nothing is headline until a test says so.
    pub fn software() -> Self {
        let mut ext = Self::new()
            .kind("behavior", true)
            .kind("invariant", true)
            .kind("feature", false)
            .reference("feature", "behaviors", "behavior")
            .reference("feature", "invariants", "invariant");
        ext.kind_mut("behavior")
            .field("contract", || Field::string("contract"));
        ext.kind_mut("invariant")
            .field("guarantee", || Field::string("guarantee"));
        ext
    }

    /// Also declare `kind` (or redeclare it, keeping its fields): a testable
    /// kind accepts `verify` obligations.
    pub fn kind(mut self, kind: &str, testable: bool) -> Self {
        self.kind_mut(kind).testable = testable;
        self
    }

    /// Entities of `kind` owe obligations: rule W004 (`no_verify_statements`,
    /// field `verify`) targets it, so one that declares none counts toward
    /// coverage instead of being exempt.
    pub fn obligating(mut self, kind: &str) -> Self {
        if !self.obligated.iter().any(|k| k == kind) {
            self.obligated.push(kind.to_string());
        }
        self
    }

    /// `contract` (normative) and `status` are headline string fields of
    /// `kind`, so the context export lifts them.
    pub fn headline(mut self, kind: &str) -> Self {
        let declared = self.kind_mut(kind);
        let contract = declared.field("contract", || Field::string("contract"));
        contract.normative = true;
        contract.headline = true;
        declared
            .field("status", || Field::string("status"))
            .headline = true;
        self
    }

    /// `kind.field` is a string field.
    pub fn string_field(mut self, kind: &str, field: &str) -> Self {
        self.kind_mut(kind).field(field, || Field::string(field));
        self
    }

    /// `kind.field` is a reference list to `target`, its edges labelled
    /// `field`: an edge the trace expects entities of `kind` to have.
    pub fn reference(mut self, kind: &str, field: &str, target: &str) -> Self {
        let declared = self
            .kind_mut(kind)
            .field(field, || Field::reference_list(field, target));
        declared.field_type = FieldType::ReferenceList;
        declared.target = Some(target.to_string());
        self
    }

    /// Every compile also reports `diagnostic`: a `check`-phase pass
    /// `report` answers it, so it joins what the compile reports as an
    /// extension pass's diagnostics do (canonical order, its `entity` as
    /// `DiagnosticData::Subject`, ADR 0016).
    pub fn reporting(mut self, diagnostic: PassDiagnostic) -> Self {
        self.reported.push(diagnostic);
        self
    }

    /// Anything else, declared with the SDK builders: a provider, an
    /// analyzer, an inference guide, a command.
    pub fn declaring(
        mut self,
        declare: impl Fn(&mut ContributionsBuilder) + Send + Sync + 'static,
    ) -> Self {
        self.declares.push(Arc::new(declare));
        self
    }

    /// The name the extension is declared under.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The extension as its guest builds it on every call.
    pub fn builder(&self) -> ContributionsBuilder {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new(&self.name, "0.1.0"));
        for kind in &self.kinds {
            c.kind(&kind.name, |k| {
                k.testable(kind.testable).supports_verify(kind.testable);
                for field in &kind.fields {
                    k.field(&field.name, |f| {
                        f.field_type(field.field_type);
                        if let Some(target) = &field.target {
                            f.target_kind(target).edge(&field.name);
                        }
                        if field.normative {
                            f.normative();
                        }
                        if field.headline {
                            f.headline();
                        }
                    });
                }
            });
        }
        // Every reference field's edges are labelled by the field: one
        // edge type per label, from whichever kinds use it.
        let mut labels: Vec<&str> = Vec::new();
        for field in self.kinds.iter().flat_map(|k| &k.fields) {
            if field.target.is_some() && !labels.contains(&field.name.as_str()) {
                labels.push(&field.name);
            }
        }
        for label in labels {
            c.edge(label, |_| {});
        }
        for kind in &self.obligated {
            c.rule("W004", |r| {
                r.check(CheckKind::NoVerifyStatements)
                    .target_kind(kind)
                    .field("verify")
                    .severity(ValidationSeverity::Warning)
                    .message_template(
                        "{kind} '{id}' is testable but declares no verify obligations",
                    );
            });
        }
        if !self.reported.is_empty() {
            let reported = self.reported.clone();
            c.pass("report", move |p| {
                p.phase("check").run(move |_| reported.clone());
            });
        }
        for declare in &self.declares {
            declare(&mut c);
        }
        c
    }

    /// The declaration the host loads (`ContributionsBuilder::declaration`).
    pub fn declaration(&self) -> ExtensionDeclaration {
        self.builder().declaration()
    }

    fn kind_mut(&mut self, kind: &str) -> &mut Kind {
        let at = match self.kinds.iter().position(|k| k.name == kind) {
            Some(at) => at,
            None => {
                self.kinds.push(Kind {
                    name: kind.to_string(),
                    testable: false,
                    fields: Vec::new(),
                });
                self.kinds.len() - 1
            }
        };
        &mut self.kinds[at]
    }
}

/// One in-process runtime serving `extensions`, each under its name.
pub fn runtime_of(extensions: &[TestExtension]) -> InProcessRuntime {
    extensions
        .iter()
        .cloned()
        .fold(InProcessRuntime::new(), |runtime, extension| {
            runtime.with(move || extension.builder())
        })
}
