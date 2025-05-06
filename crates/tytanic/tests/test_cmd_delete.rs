mod fixture;

#[test]
fn test_delete() {
    let env = fixture::Environment::default_package();
    let res = env.run_tytanic(["delete", "failing/compile"]);

    insta::assert_snapshot!(res.output(), @r"
    --- CODE: 0
    --- STDOUT:

    --- STDERR:
    Deleted 1 test

    --- END
    ");
}

#[test]
fn test_delete_not_found() {
    let env = fixture::Environment::default_package();
    let res = env.run_tytanic(["delete", "foo"]);

    insta::assert_snapshot!(res.output(), @r"
    --- CODE: 2
    --- STDOUT:

    --- STDERR:
    error: Test foo not found

    --- END
    ");
}

#[test]
fn test_new_delete_alias() {
    let env = fixture::Environment::default_package();
    let res = env.run_tytanic(["remove", "failing/compile"]);

    insta::assert_snapshot!(res.output(), @r"
    --- CODE: 0
    --- STDOUT:

    --- STDERR:
    warning: Sub command alias remove|rm is deprecated
    hint: Use delete instead
    Deleted 1 test

    --- END
    ");
}
