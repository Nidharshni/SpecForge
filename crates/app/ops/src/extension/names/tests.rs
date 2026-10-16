use super::*;
use crate::OpErrorKind;
use crate::testing::{self, GREET};
use specforge_test_macros::test as specforge_test;
use specforge_wasm::testing::InProcessRuntime;
use std::path::PathBuf;

fn runtime() -> InProcessRuntime {
    testing::candidates()
}

/// A place whose root and base are `dir`, unbounded.
fn place<'r>(dir: &'r Path, containment: &'r Containment) -> Place<'r> {
    Place {
        root: dir,
        base: dir,
        containment,
    }
}

static UNBOUNDED: Containment = Containment::unbounded();

/// Every string refused as a registry package: no registry is asked in these tests.
fn no_packages(_: &str, _: &PackageRef) -> Result<Version, OpError> {
    panic!("no package is asked")
}

fn write_greet(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, GREET).unwrap();
}

#[test]
fn parse_names_each_source() {
    assert_eq!(
        parse("@specforge/product"),
        Ok(Source::Builtin("@specforge/product"))
    );
    assert_eq!(
        parse("ext/greet.wasm"),
        Ok(Source::Local(PathBuf::from("ext/greet.wasm")))
    );
    assert_eq!(
        parse("./ext/dir"),
        Ok(Source::Local(PathBuf::from("./ext/dir")))
    );
    assert_eq!(
        parse("@acme/tool@^1.2"),
        Ok(Source::Registry(
            PackageRef::parse("@acme/tool@^1.2").unwrap()
        ))
    );
    assert_eq!(
        parse("git+https://example.com/x.git"),
        Ok(Source::Git {
            url: "https://example.com/x.git".into()
        })
    );
    assert_eq!(parse("").unwrap_err().code, "E054");
    // Registry packages are always `@scope/name`: anything else names a path.
    assert_eq!(parse("not-scoped"), Ok(Source::Local("not-scoped".into())));
    assert_eq!(parse("@acme/").unwrap_err().code, "E054");
}

/// What `parse` makes of every input of plan 12's table (§2.2): a
/// package reference, or the code of the refusal. Nothing reaches a
/// registry that is not a scoped package name with a requirement.
#[specforge_test(
    behavior = "parse_extension_specifier",
    verify = "each add argument reads as one extension source"
)]
fn parse_reads_each_input() {
    fn registry(reference: &str) -> Result<Source, String> {
        Ok(Source::Registry(PackageRef::parse(reference).unwrap()))
    }
    fn local(path: &str) -> Result<Source, String> {
        Ok(Source::Local(PathBuf::from(path)))
    }
    let cases: Vec<(&str, Result<Source, String>)> = vec![
        ("@acme/tool", registry("@acme/tool")),                 // I1
        ("@acme/tool@", Err("E054".into())),                    // I2
        ("@acme/tool@1.2.0", registry("@acme/tool@1.2.0")),     // I3
        ("@acme/tool@^1.2", registry("@acme/tool@^1.2")),       // I4
        ("@acme/tool@1.x", registry("@acme/tool@1.x")),         // I5
        ("@acme/tool@1.2", registry("@acme/tool@1.2")),         // I6
        ("@acme/tool@1.0.0/x", Err("R-RES-003".into())),        // I7
        ("@acme/tool@1.0.0?x=1", Err("R-RES-003".into())),      // I8
        ("foo@/bar", local("foo@/bar")),                        // I9
        ("tool@1.0.0", local("tool@1.0.0")),                    // I10
        ("tool", local("tool")),                                // I11
        ("@acme/..", Err("E054".into())),                       // I12
        ("@acme/aa/bb", Err("E054".into())),                    // I13
        ("@acme/a/b", Err("E054".into())),                      // I14
        ("@a/x", registry("@a/x")),                             // I15
        ("@acme/T ool", Err("E054".into())),                    // I16
        ("Acme@1", local("Acme@1")),                            // I17
        ("@acme/tool@latest", registry("@acme/tool")),          // I18
        ("@acme/tool@*", registry("@acme/tool")),               // I18
        ("@acme/tool@>=1, <2", registry("@acme/tool@>=1, <2")), // I19
        ("@acme/tool@^bogus", Err("R-RES-003".into())),         // I20
        (
            "@acme/tool@2.0.0+build.1",
            registry("@acme/tool@2.0.0+build.1"),
        ), // I21
        ("@scope", Err("E054".into())),                         // I22
        (
            "@specforge/software",
            Ok(Source::Builtin("@specforge/software")),
        ), // I24
        (
            "git+https://h/r#v",
            Ok(Source::Git {
                url: "https://h/r".into(),
            }),
        ), // I25
        (" @acme/tool ", registry("@acme/tool")),               // I26
    ];
    for (input, want) in cases {
        let got = parse(input).map_err(|error| error.code.to_string());
        assert_eq!(got, want, "{input:?}");
    }
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "the specifiers of an add are read in order, the first unreadable one refused and named among several"
)]
fn each_string_is_read_in_order_and_refused_named_among_several() {
    let dir = Path::new("/p");
    let read = Names::read(
        &["@specforge/product", "@acme/tool@^1"],
        place(dir, &UNBOUNDED),
    )
    .unwrap();
    assert_eq!(
        read.wanted,
        [
            Specified {
                specifier: "@specforge/product".into(),
                source: Source::Builtin("@specforge/product"),
            },
            Specified {
                specifier: "@acme/tool@^1".into(),
                source: Source::Registry(PackageRef::parse("@acme/tool@^1").unwrap()),
            },
        ]
    );

    let error = Names::read(
        &["@specforge/product", "@acme/tool@", "@acme/"],
        place(dir, &UNBOUNDED),
    )
    .err()
    .unwrap();
    assert_eq!(error.code, "E054");
    assert!(
        error
            .message
            .starts_with("adding '@acme/tool@': invalid extension specifier: "),
        "{error:?}"
    );
    let alone = Names::read(&["@acme/"], place(dir, &UNBOUNDED))
        .err()
        .unwrap();
    assert!(
        alone.message.starts_with("invalid extension specifier: "),
        "{alone:?}"
    );
    let none = Names::read::<&str>(&[], place(dir, &UNBOUNDED)).unwrap();
    assert!(none.wanted.is_empty());
    // Nothing splits a string: a comma is part of the specifier.
    let comma = Names::read(
        &["@specforge/product,@specforge/governance"],
        place(dir, &UNBOUNDED),
    )
    .err()
    .unwrap();
    assert_eq!(comma.code, "E054");
}

#[test]
fn a_relative_local_path_is_joined_onto_the_base() {
    let runtime = runtime();
    let scratch = tempfile::tempdir().unwrap();
    let base = scratch.path().join("sub");
    write_greet(&base.join("a.wasm"));
    let place = Place {
        root: scratch.path(),
        base: &base,
        containment: &UNBOUNDED,
    };
    let names = Names::read(&["a.wasm", "/abs/b.wasm"], place).unwrap();
    assert_eq!(names.wanted[0].source, Source::Local(base.join("a.wasm")),);
    assert_eq!(names.wanted[1].source, Source::Local("/abs/b.wasm".into()));
    let named = names.name_each(&runtime, &mut no_packages).err();
    // The second file does not exist: refused named among several, naming the path as given.
    let error = named.unwrap();
    assert_eq!(error.code, "E054");
    assert_eq!(
        error.message,
        "adding '/abs/b.wasm': file not found: /abs/b.wasm"
    );
}

#[cfg(unix)]
#[test]
fn a_dot_dot_follows_the_file_system_where_the_path_exists() {
    let runtime = runtime();
    let scratch = tempfile::tempdir().unwrap();
    let dir = scratch.path().join("p");
    std::fs::create_dir_all(scratch.path().join("elsewhere/sub")).unwrap();
    write_greet(&scratch.path().join("elsewhere/a.wasm"));
    std::fs::create_dir_all(&dir).unwrap();
    std::os::unix::fs::symlink(scratch.path().join("elsewhere/sub"), dir.join("link")).unwrap();

    let names = Names::read(&["link/../a.wasm"], place(&dir, &UNBOUNDED)).unwrap();
    let named = names.name_each(&runtime, &mut no_packages).unwrap();

    let [Named::Local { install, .. }] = named.as_slice() else {
        panic!("one local file");
    };
    assert_eq!(install.name().as_str(), "@sdk/greet");
}

#[test]
fn a_dot_dot_out_of_a_directory_not_made_yet_resolves_by_name() {
    let runtime = runtime();
    let scratch = tempfile::tempdir().unwrap();
    write_greet(&scratch.path().join("a.wasm"));
    let dir = scratch.path().join("notyet");

    let names = Names::read(&["../a.wasm"], place(&dir, &UNBOUNDED)).unwrap();
    let named = names.name_each(&runtime, &mut no_packages).unwrap();

    assert!(matches!(named.as_slice(), [Named::Local { .. }]));
}

#[test]
fn two_spellings_of_one_file_are_one_install() {
    let runtime = runtime();
    let scratch = tempfile::tempdir().unwrap();
    write_greet(&scratch.path().join("a.wasm"));
    std::fs::create_dir_all(scratch.path().join("sub")).unwrap();

    let names = Names::read(
        &["a.wasm", "sub/../a.wasm"],
        place(scratch.path(), &UNBOUNDED),
    )
    .unwrap();
    let named = names.name_each(&runtime, &mut no_packages).unwrap();

    assert_eq!(named.len(), 2);
    assert!(matches!(named[0], Named::Local { .. }));
    let Named::Duplicate { name, first } = &named[1] else {
        panic!("the second names it the same way");
    };
    assert_eq!((name.as_str(), first.as_str()), ("@sdk/greet", "a.wasm"));
}

#[test]
fn a_package_is_resolved_by_the_callers_step_then_claimed() {
    let runtime = runtime();
    let dir = Path::new("/p");
    let step = |_: &str, package: &PackageRef| {
        let version = if package.to_string().contains("@3") {
            "2.0.0"
        } else {
            "1.0.0"
        };
        Ok(Version::parse(version).unwrap())
    };
    let names = Names::read(&["@x/b", "@x/b@1.0.0"], place(dir, &UNBOUNDED)).unwrap();
    let named = names.name_each(&runtime, &mut { step }).unwrap();
    assert!(matches!(&named[0], Named::Package { name, version }
        if name.as_str() == "@x/b" && version.to_string() == "1.0.0"));
    assert!(matches!(&named[1], Named::Duplicate { name, .. } if name == "@x/b"));

    let names = Names::read(&["@x/b", "@x/b@3.0.0"], place(dir, &UNBOUNDED)).unwrap();
    let error = names.name_each(&runtime, &mut { step }).err().unwrap();
    assert_eq!(error.code, "extension_conflict");
    assert_eq!(error.kind, OpErrorKind::Conflict);
}

#[test]
fn a_git_source_is_e064_named_among_several() {
    let runtime = runtime();
    let dir = Path::new("/p");
    let names = Names::read(
        &["@specforge/product", "git+https://x/y"],
        place(dir, &UNBOUNDED),
    )
    .unwrap();
    let error = names.name_each(&runtime, &mut no_packages).err().unwrap();
    assert_eq!(error.code, "E064");
    assert!(
        error.message.starts_with("adding 'git+https://x/y': "),
        "{error:?}"
    );
}

#[test]
fn a_local_file_outside_the_containment_is_refused_at_the_read() {
    let runtime = runtime();
    let scratch = tempfile::tempdir().unwrap();
    let dir = scratch.path().join("p");
    std::fs::create_dir_all(&dir).unwrap();
    write_greet(&scratch.path().join("out.wasm"));
    let bound = Containment::within(&dir);

    let names = Names::read(&["../out.wasm"], place(&dir, &bound)).unwrap();
    let error = names.name_each(&runtime, &mut no_packages).err().unwrap();

    assert_eq!(error.code, crate::OUTSIDE_ROOT);
}
