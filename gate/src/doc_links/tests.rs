use super::*;

fn tracked(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|p| p.to_string()).collect()
}

fn check(paths: &[&str], files: &[(&str, &str)]) -> Vec<Broken> {
    broken(&tracked(paths), |path| {
        files
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, text)| text.to_string())
    })
}

#[test]
fn a_link_to_a_tracked_file_or_directory_holds() {
    let paths = ["README.md", "docs/a.md", "crates/x/src/lib.rs"];
    let readme = "[a](docs/a.md) [c](crates/) [x](crates/x) [self](./README.md)";
    assert_eq!(check(&paths, &[("README.md", readme)]), vec![]);
}

#[test]
fn a_link_to_an_untracked_path_is_reported_with_its_line() {
    let paths = ["README.md"];
    let found = check(&paths, &[("README.md", "# Title\n[gone](plans/MASTER.md)")]);
    assert_eq!(
        found,
        vec![Broken {
            file: "README.md".into(),
            line: 2,
            target: "plans/MASTER.md".into()
        }]
    );
    assert_eq!(
        found[0].to_string(),
        "README.md:2 links `plans/MASTER.md`, which is not tracked"
    );
}

#[test]
fn urls_anchors_code_spans_and_fences_are_not_judged() {
    let text = "[u](https://x.org/a) [m](mailto:a@b.c) [h](#top) [p](//cdn.x/y) `[c](nope.md)`\n\
                ```\n[f](nope.md)\n```\n~~~\n[t](nope.md)\n~~~\n`[x := E]R = [S]([T]R)`";
    assert_eq!(check(&["a.md"], &[("a.md", text)]), vec![]);
}

#[test]
fn a_link_resolves_from_its_file_and_from_the_root() {
    let paths = ["README.md", "docs/vision/p.md", "docs/guides/g.md"];
    let text = "[p](../vision/p.md) [r](/README.md) [up](../../../x.md)";
    let found = check(&paths, &[("docs/guides/g.md", text)]);
    assert_eq!(
        found.iter().map(|b| b.target.as_str()).collect::<Vec<_>>(),
        ["../../../x.md"]
    );
}

#[test]
fn fragments_queries_titles_and_html_attributes_are_read() {
    let paths = ["a.md", "docs/a.md", "assets/x.svg"];
    let text = "[a](docs/a.md#part \"Title\") [q](docs/a.md?plain=1) <img src=\"assets/x.svg\"> \
                <a href=\"missing.md\">m</a> [b](<docs/a.md>)";
    let found = check(&paths, &[("a.md", text)]);
    assert_eq!(
        found.iter().map(|b| b.target.as_str()).collect::<Vec<_>>(),
        ["missing.md"]
    );
}

#[test]
fn only_markdown_files_are_read() {
    let paths = ["page.html", "notes.txt"];
    let found = broken(&tracked(&paths), |path| panic!("read {path}"));
    assert_eq!(found, vec![]);
}
