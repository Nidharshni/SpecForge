use super::*;

#[specforge_test_macros::test(
    behavior = "load_extension_declaration",
    verify = "a short name that is not lowercase kebab case is refused when the extension is built"
)]
fn a_short_name_that_is_not_kebab_case_is_refused() {
    for bad in [
        "Friendly greetings",
        "Greet",
        "greet_x",
        "-greet",
        "1greet",
        "",
    ] {
        let attr = quote! { name = "@sdk/greet", short = #bad };
        let error = parse_attrs(attr).expect_err(bad);
        assert!(
            error.to_string().contains("is not lowercase kebab case"),
            "{bad}: {error}"
        );
    }
    let attr = quote! {
        name = "@sdk/greet", version = "0.1.0", short = "greet-2",
        description = "Friendly greetings"
    };
    assert_eq!(
        parse_attrs(attr).unwrap(),
        Attrs {
            name: Some("@sdk/greet".to_string()),
            version: Some("0.1.0".to_string()),
            short: Some("greet-2".to_string()),
            description: Some("Friendly greetings".to_string()),
        }
    );
}

#[test]
fn an_unknown_attribute_is_refused() {
    let error = parse_attrs(quote! { name = "@a/b", starter = "x" }).unwrap_err();
    assert!(
        error.to_string().contains("unsupported property"),
        "{error}"
    );
}
