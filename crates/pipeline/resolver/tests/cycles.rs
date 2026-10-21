//! W113: the files on an import cycle, however the cycle closes (ADR 0196).

use specforge_parser::parse;
use specforge_resolver::resolve_imports;
use specforge_test_macros::test as specforge_test;

#[specforge_test(
    behavior = "detect_import_cycles",
    verify = "W113 names every file of an import cycle, one whose cycle closes through a file already walked included"
)]
fn w113_names_every_file_of_an_import_cycle() {
    let files = [
        ("aa.spec", "use \"bb\"\nuse \"dd\"\nterm a \"A\" { }"),
        ("bb.spec", "use \"cc\"\nterm b \"B\" { }"),
        ("cc.spec", "use \"aa\"\nterm c \"C\" { }"),
        ("dd.spec", "use \"bb\"\nterm d \"D\" { }"),
    ];
    let parsed: Vec<_> = files.iter().map(|(k, t)| (*k, parse(t, k))).collect();
    let refs: Vec<_> = parsed.iter().map(|(k, f)| (*k, f)).collect();

    let diagnostics = resolve_imports(&refs, &[]);

    let messages: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == "W113")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "circular import detected: aa.spec -> bb.spec -> cc.spec -> aa.spec -> dd.spec -> bb.spec -> cc.spec"
        ]
    );
}
