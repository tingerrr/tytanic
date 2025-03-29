use self::fixture::assert_fs;

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

    assert_fs!(env.root() => [
        "tests" => [
            "foo" => [
                "ref" => is_dir,
                "test.typ" => is_file,
            ]
        ]
    ]);
}

#[test]
fn test_new_conflict() {
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

    assert_fs!(env.root() => [
        "tests" => [
            "foo" => [
                "ref" => is_dir,
                "test.typ" => is_file,
            ]
        ]
    ]);

    let res = env.run_tytanic(["add", "foo"]);

    insta::assert_snapshot!(res.output(), @r"
    --- CODE: 2
    --- STDOUT:

    --- STDERR:
    warning: Sub command alias add is deprecated
    hint: Use new instead
    error: Test foo already exists

    --- END
    ");

    assert_fs!(env.root() => [
        "tests" => [
            "foo" => [
                "ref" => is_dir,
                "test.typ" => is_file,
            ]
        ]
    ]);
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

    assert_fs!(env.root() => [
        "tests" => [
            "foo" => [
                "ref" => is_dir,
                "test.typ" => is_file,
            ]
        ]
    ]);
}
