mod fixture;

#[test]
fn test_status() {
    let env = fixture::Environment::default_package();
    let res = env.run_tytanic(["list"]);

    insta::assert_snapshot!(res.output(), @r"
    --- CODE: 0
    --- STDOUT:

    --- STDERR:
    @template                          template    
    failing/compile                    compile-only
    failing/ephemeral-compare-failure  ephemeral   
    failing/ephemeral-compile-failure  ephemeral   
    failing/persistent-compare-failure persistent  
    failing/persistent-compile-failure persistent  
    passing/compile                    compile-only
    passing/ephemeral                  ephemeral   
    passing/persistent                 persistent  

    --- END
    ");
}
