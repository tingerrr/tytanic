mod fixture;

#[test]
fn test_root_empty_package() {
    let env = fixture::Environment::default_package();
    let res = env.run_tytanic(["--root", ".", "status"]);

    insta::assert_snapshot!(res.output(), @r"
    --- CODE: 0
    --- STDOUT:

    --- STDERR:
     Project ┌ template:0.1.0
         Vcs ├ none
    Template ├ tests/template.typ
       Tests ├ 3 persistent
             ├ 3 ephemeral
             └ 2 compile-only

    --- END
    ");
}

#[test]
fn test_root_empty() {
    let env = fixture::Environment::new();
    let res = env.run_tytanic(["--root", ".", "status"]);

    insta::assert_snapshot!(res.output(), @r"
    --- CODE: 0
    --- STDOUT:

    --- STDERR:
     Project ┌ none
         Vcs ├ none
    Template ├ none
       Tests └ none

    --- END
    ");
}
