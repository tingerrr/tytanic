use assert_fs::assert::PathAssert;
use assert_fs::prelude::PathChild;

mod fixture;

// TODO(tinger): Use tytanic_utils::fs for filesystem assertions.

#[test]
fn test_new() {
    let env = fixture::Environment::empty_package();
    let res = env.run_tytanic(["new", "foo"]);

    let tests = env.dir().child("tests");
    let foo = tests.child("foo");
    let ref_ = foo.child("ref");
    let test = foo.child("test.typ");

    tests.assert(predicates::path::is_dir());
    foo.assert(predicates::path::is_dir());
    ref_.assert(predicates::path::is_dir());
    test.assert(predicates::path::is_file());

    insta::assert_snapshot!(res.output(), @r"
    --- CODE: exit status: 0
    --- STDOUT:

    --- STDERR:
    Added foo

    --- END
    ");
}
