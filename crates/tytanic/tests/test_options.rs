use assert_fs::assert::PathAssert;
use assert_fs::prelude::PathChild;

mod fixture;

#[test]
fn test_root_empty_package() {
    let env = fixture::Environment::empty_package();
    let res = env.run_tytanic(["--root", ".", "status"]);

    insta::assert_snapshot!(res.output(), @r"
    --- CODE: exit status: 0
    --- STDOUT:

    --- STDERR:
     Project ┌ my-package:0.1.0
         Vcs ├ none
    Template ├ none
       Tests └ none

    --- END
    ");
}

#[test]
fn test_root_empty() {
    let env = fixture::Environment::new();
    let res = env.run_tytanic(["--root", ".", "status"]);

    insta::assert_snapshot!(res.output(), @r"
    --- CODE: exit status: 0
    --- STDOUT:

    --- STDERR:
     Project ┌ none
         Vcs ├ none
    Template ├ none
       Tests └ none

    --- END
    ");
}
