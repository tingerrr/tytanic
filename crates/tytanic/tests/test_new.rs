use assert_fs::{assert::PathAssert, prelude::PathChild};

mod fixture;

#[test]
fn test_new_with_root() {
    let res = fixture::TestBuilder::new_with_args(["--root", ".", "new", "foo"]).run();

    let tests = res.dir().child("tests");
    let foo = tests.child("foo");
    let ref_ = foo.child("ref");
    let test = foo.child("test.typ");

    tests.assert(predicates::path::is_dir());
    foo.assert(predicates::path::is_dir());
    ref_.assert(predicates::path::is_dir());
    test.assert(predicates::path::is_file());

    insta::assert_snapshot!(res.output().stdout(), @"");
    insta::assert_snapshot!(res.output().stderr(), @"Added foo");
}
