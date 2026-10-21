//! `check` and `product module-depth` name the same modules on a cycle (ADR 0196).

use std::collections::BTreeSet;

use crate::support::{Cli, TestProject};
use specforge_test_macros::test as specforge_test;

const TANGLE: &str = "module alpha \"A\" {\n  depends_on [beta, delta]\n}\n\n\
module beta \"B\" {\n  depends_on [gamma]\n}\n\n\
module gamma \"G\" {\n  depends_on [alpha]\n}\n\n\
module delta \"D\" {\n  depends_on [beta]\n}\n";

fn names<'a>(ids: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
    ids.into_iter().map(String::from).collect()
}

#[specforge_test(
    behavior = "detect_module_cycles",
    verify = "E007, W061 and module-depth name the same modules on a cycle"
)]
fn check_and_module_depth_name_the_same_modules_on_a_cycle() {
    let dir = TestProject::new()
        .enabling(&["@specforge/product"])
        .file("spec/m.spec", TANGLE);
    let cli = Cli::on(&dir);

    let check = cli.document(&["check"]).json;
    let diagnostics = check.as_array().unwrap();
    let of = |code: &'static str| diagnostics.iter().filter(move |d| d["code"] == code);
    let e007 = names(of("E007").map(|d| {
        let message = d["message"].as_str().unwrap();
        message.split('\'').nth(1).unwrap()
    }));
    let w061 = names(
        of("W061")
            .flat_map(|d| d["data"]["path"].as_array().unwrap())
            .map(|id| id.as_str().unwrap()),
    );
    let depth = cli.document(&["product", "module-depth", "delta"]).json;
    let chain = names(
        depth["longest_chain"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_str().unwrap()),
    );

    let all = names(["alpha", "beta", "delta", "gamma"]);
    assert_eq!(chain, all);
    assert_eq!(w061, all, "W061");
    assert_eq!(e007, all, "E007");
}
