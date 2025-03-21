use toml::{Table, Value};

fn main() {
    let lock: Table = toml::from_str(include_str!("../../Cargo.lock")).unwrap();
    let packages = lock["package"].as_array().unwrap();
    let typst = packages
        .iter()
        .filter_map(Value::as_table)
        .find(|t| {
            t.get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| n == "typst")
        })
        .unwrap();
    let typst_version = typst["version"].as_str().unwrap();

    println!("cargo::rustc-env=TYTANIC_TYPST_VERSION={}", typst_version);
}
