mod fixture;

#[test]
fn test_root_no_manifest() {
    let env = fixture::Environment::new();
    let res = env.run_tytanic(["status"]);

    insta::assert_snapshot!(res.output(), @r"
    --- CODE: 2
    --- STDOUT:

    --- STDERR:
    error: Must be in a typst project
    hint: You can pass the project root using --root <path>

    --- END
    ");

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
