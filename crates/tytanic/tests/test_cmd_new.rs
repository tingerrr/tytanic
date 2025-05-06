mod fixture;

#[test]
fn test_new() {
    let env = fixture::Environment::default_package();
    let res = env.run_tytanic(["new", "foo"]);

    insta::assert_snapshot!(res.output(), @r"
    --- CODE: 0
    --- STDOUT:

    --- STDERR:
    Added foo

    --- END
    ");
}

#[test]
fn test_new_conflict() {
    let env = fixture::Environment::default_package();
    let res = env.run_tytanic(["new", "passing/compile"]);

    insta::assert_snapshot!(res.output(), @r"
    --- CODE: 2
    --- STDOUT:

    --- STDERR:
    error: Test passing/compile already exists

    --- END
    ");
}

#[test]
fn test_new_add_alias() {
    let env = fixture::Environment::default_package();
    let res = env.run_tytanic(["add", "foo"]);

    insta::assert_snapshot!(res.output(), @r"
    --- CODE: 0
    --- STDOUT:

    --- STDERR:
    warning: Sub command alias add is deprecated
    hint: Use new instead
    Added foo

    --- END
    ");
}
