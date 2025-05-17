//! Standard library augmentation, i.e. additional functions and values for the
//! typst standard library.
//!
//! # Functions
//! ## `catch`
//! Provides a mechanism to catch panics inside test scripts. Returns an array
//! of strings for each panic.
//! ```typst
//! #let (msg,) = catch(() => {
//!   panic()
//! })
//! ```
//!
//! ## `assert-panic`
//! Provides an assertion that tests if a given closure panicked, panicking if
//! it did not. Takes an optional `message` similar to other `assert` functions.
//! ```typst
//! #assert-panic(() => {}, message: "Did not panic")
//! ```

use comemo::Tracked;
use ecow::EcoString;
use typst::diag::bail;
use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::func;
use typst::foundations::Context;
use typst::foundations::Func;
use typst::foundations::Module;
use typst::foundations::Repr;
use typst::foundations::Scope;
use typst::foundations::Str;
use typst::foundations::Value;
use typst::Library;
use typst::LibraryBuilder;

/// Defines prelude items for the given scope, this is a subset of
/// [`define_test_module`].
pub fn define_prelude(scope: &mut Scope) {
    scope.define_func::<catch>();
    scope.define_func::<assert_panic>();
}

/// Defines test module items for the given scope.
pub fn define_test_module(scope: &mut Scope) {
    define_prelude(scope)
}

/// Creates a new test module with the items defined by [`define_test_module`].
pub fn test_module() -> Module {
    let mut scope = Scope::new();
    define_test_module(&mut scope);
    Module::new("test", scope)
}

/// Creates a new augmented default standard library. See [`augmented_library`].
pub fn augmented_default_library() -> Library {
    augmented_library(|x| x)
}

/// Creates a new augmented standard library, applying the given closure to the
/// builder.
///
/// The augmented standard library contains a new test module and a few items in
/// the prelude for easier testing.
pub fn augmented_library(builder: impl FnOnce(LibraryBuilder) -> LibraryBuilder) -> Library {
    let mut lib = builder(LibraryBuilder::default()).build();
    let scope = lib.global.scope_mut();

    scope.define("test", test_module());
    define_prelude(scope);

    lib
}

#[func]
fn catch(engine: &mut Engine, context: Tracked<Context>, func: Func) -> Value {
    func.call::<[Value; 0]>(engine, context, [])
        .map(|_| Value::None)
        .unwrap_or_else(|errors| {
            Value::Str(Str::from(
                errors
                    .first()
                    .expect("should contain at least one diagnostic")
                    .message
                    .clone(),
            ))
        })
}

#[func]
fn assert_panic(
    engine: &mut Engine,
    context: Tracked<Context>,
    func: Func,
    #[named] message: Option<EcoString>,
) -> SourceResult<()> {
    let result = func.call::<[Value; 0]>(engine, context, []);
    let span = func.span();
    if let Ok(val) = result {
        match message {
            Some(message) => bail!(span, "{}", message),
            None => match val {
                Value::None => bail!(span, "Expected panic, closure returned successfully"),
                _ => bail!(
                    span,
                    "Expected panic, closure returned successfully with {}",
                    val.repr(),
                ),
            },
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use typst::syntax::Source;

    use super::*;
    use crate::doc::compile;
    use crate::doc::compile::Warnings;
    use crate::world_builder::file::VirtualFileProvider;
    use crate::world_builder::library::LibraryProvider;
    use crate::world_builder::test_utils;

    #[test]
    fn test_catch() {
        let mut files = VirtualFileProvider::new();
        let library = LibraryProvider::with_library(augmented_default_library());

        let source = Source::detached(
            r#"
            #let errors = catch(() => {
                panic()
            })
            #assert.eq(errors, "panicked")
        "#,
        );

        let world = test_utils::virtual_world(source.clone(), &mut files, &library);

        compile::compile(source, &world, Warnings::Emit)
            .output
            .unwrap();
    }

    #[test]
    fn test_assert_panic() {
        let mut files = VirtualFileProvider::new();
        let library = LibraryProvider::with_library(augmented_default_library());

        let source = Source::detached(
            r#"
            #assert-panic(() => {
                panic()
            })
        "#,
        );

        let world = test_utils::virtual_world(source.clone(), &mut files, &library);

        compile::compile(source, &world, Warnings::Emit)
            .output
            .unwrap();
    }
}
