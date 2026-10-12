#[specforge_test_macros::test(
    invariant = "task_id_uniqueness",
    verify = "concurrent task creation never produces duplicate ids"
)]
fn declared_text() {}

#[spec(
    invariant = "task_id_uniqueness",
    verify = "concurrent task creation never produces duplicate id"
)]
fn undeclared_text() {}

#[spec(invariant = "no_such_invariant")]
fn unknown_entity() {}
