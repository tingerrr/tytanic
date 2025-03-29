mod fixture;

#[test]
fn test_status() {
    let env = fixture::Environment::default_package();
    let res = env.run_tytanic(["status"]);

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
fn test_status_json() {
    let env = fixture::Environment::default_package();
    let res = env.run_tytanic(["status", "--json"]);

    insta::assert_snapshot!(res.output(), @r#"
    --- CODE: 0
    --- STDOUT:
    {
      "package": {
        "name": "template",
        "version": "0.1.0"
      },
      "vcs": null,
      "tests": [
        {
          "id": "failing/compile",
          "kind": "compile-only",
          "is_skip": false,
          "path": "<TEMP_DIR>/tests/failing/compile"
        },
        {
          "id": "failing/ephemeral-compare-failure",
          "kind": "ephemeral",
          "is_skip": false,
          "path": "<TEMP_DIR>/tests/failing/ephemeral-compare-failure"
        },
        {
          "id": "failing/ephemeral-compile-failure",
          "kind": "ephemeral",
          "is_skip": false,
          "path": "<TEMP_DIR>/tests/failing/ephemeral-compile-failure"
        },
        {
          "id": "failing/persistent-compare-failure",
          "kind": "persistent",
          "is_skip": false,
          "path": "<TEMP_DIR>/tests/failing/persistent-compare-failure"
        },
        {
          "id": "failing/persistent-compile-failure",
          "kind": "persistent",
          "is_skip": false,
          "path": "<TEMP_DIR>/tests/failing/persistent-compile-failure"
        },
        {
          "id": "passing/compile",
          "kind": "compile-only",
          "is_skip": false,
          "path": "<TEMP_DIR>/tests/passing/compile"
        },
        {
          "id": "passing/ephemeral",
          "kind": "ephemeral",
          "is_skip": false,
          "path": "<TEMP_DIR>/tests/passing/ephemeral"
        },
        {
          "id": "passing/persistent",
          "kind": "persistent",
          "is_skip": false,
          "path": "<TEMP_DIR>/tests/passing/persistent"
        }
      ],
      "template_test": {
        "id": "@template",
        "path": "<TEMP_DIR>/template"
      }
    }
    --- STDERR:

    --- END
    "#);
}
