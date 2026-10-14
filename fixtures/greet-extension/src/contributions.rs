//! What the greet extension declares, authored entirely with the SpecForge
//! extension SDK. Kept apart from the component glue (`lib.rs`) so the
//! host's tests can serve the same declarations in process
//! (`crates/runtime/component/tests/greet_sdk.rs` includes this file) and
//! check both runtimes answer alike.

use specforge_extension_sdk::prelude::*;

#[specforge_extension_sdk::extension(
    name = "@sdk/greet",
    version = "0.1.0",
    short = "greet",
    description = "Friendly greetings"
)]
pub struct Greet;

/// The greeting styles.
const STYLES: Vocabulary = Vocabulary::new(&["warm", "formal"]);

impl Contributions for Greet {
    fn contribute(c: &mut ContributionsBuilder) {
        c.kind("greeting", |k| {
            k.description("A friendly greeting").testable(false);
            k.field("style", |f| {
                f.vocabulary(&STYLES).required();
            });
        });
        c.rule("E901", |r| {
            r.check(CheckKind::FieldValueConstraint);
            r.target_kind("greeting");
            r.field("style");
            r.constraint(|fc| {
                fc.kind(ConstraintKind::Matches);
                fc.pattern("^(warm|formal)$");
            });
            r.severity(ValidationSeverity::Error);
            r.message_template("greeting '{id}' has unknown style");
        });
        c.command("hello", |cmd| {
            cmd.title("Say hello")
                .description("Greet someone, warmly")
                .arg("name", |a| {
                    a.string().required().description("Who to greet");
                })
                .handler(|call| {
                    let name = call.str("name").unwrap_or_default();
                    let greeting = format!("Hello, {name}!");
                    call.render(&serde_json::json!({ "greeting": greeting }), |out| {
                        out.push_str(&greeting);
                        out.push('\n');
                    })
                });
        });
        c.pass("styles", |p| {
            p.run(|input: &PassInput| {
                input
                    .entities
                    .iter()
                    .filter(|e| e.kind == "greeting")
                    .map(|e| {
                        let style = e.fields.get("style").map_or("none", String::as_str);
                        PassDiagnostic::new(
                            "G900",
                            PassSeverity::Info,
                            format!("greeting '{}' is {style}", e.id),
                        )
                        .with_entity(&e.id)
                    })
                    .collect::<Vec<_>>()
            });
        });
        c.collector("greet-test", |k| {
            k.input_format("greet-lines")
                .report("greet-report.txt")
                .collect(|input: &CollectInput| {
                    // One line per test: `<greeting id> <passed|failed|skipped>`.
                    let entity_results = input
                        .reports
                        .iter()
                        .flat_map(|report| {
                            report
                                .content
                                .lines()
                                .filter_map(|line| line.split_once(' '))
                                .map(move |(id, status)| (report, id, status.trim()))
                        })
                        .map(|(report, id, status)| {
                            let status = serde_json::from_value::<TestStatus>(
                                serde_json::Value::String(status.to_string()),
                            )
                            .map_err(|_| {
                                format!(
                                    "{}: '{status}' is not passed, failed or skipped",
                                    report.path
                                )
                            })?;
                            Ok(CollectEntityResult {
                                entity_id: id.to_string(),
                                test_results: vec![TestResult {
                                    name: format!("greets_{id}"),
                                    status,
                                    verify: None,
                                    duration_ms: None,
                                }],
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    Ok(CollectOutput {
                        entity_results,
                        unlinked: Vec::new(),
                    })
                });
        });
    }
}

/// The extension's contributions, as its guest builds them per call.
pub fn build() -> ContributionsBuilder {
    specforge_extension_build()
}
