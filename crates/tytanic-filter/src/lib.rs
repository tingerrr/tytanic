//! A functional set-based DSL for filtering tests in the `tytanic` test runner.
//! See the language [reference] and [guide] for more info.
//!
//! Note that this is generic over the test type because it is also used in
//! internal projects of the author. This means that contribution is welcome,
//! but may be rejected for various reasons not apparent in `tytanic` only.
//!
//! This libary is still unstable to some degree, the inner test set types are
//! very opaque and not easily printed or inspected because of this, this may
//! change in the future.
//!
//! [reference]: https://typst-community.github.io/tytanic/reference/test-sets/index.html
//! [guide]: https://typst-community.github.io/tytanic/guides/test-sets.html

use ecow::EcoString;
use eval::Value;
use thiserror::Error;

use crate::eval::{Eval, Test};

pub mod ast;
pub mod eval;

/// A generic test set expression filter, this filter checks whether a test
/// should be filtered out by checking it against the inner test set within its
/// evaluation context.
///
/// This also includes extra parsing logic for the special `all:` modifier
/// prefix, which is not part of the test set grammar, but can be used by the
/// caller to handle instances where multiple tests match but only one is
/// usually expected.
#[derive(Debug, Clone)]
pub struct ExpressionFilter<T: 'static> {
    input: EcoString,
    all: bool,
    ctx: eval::Context<T>,
    set: eval::Set<T>,
}

impl<T: Test> ExpressionFilter<T> {
    /// Parse and evaluate a string into a test set with the given context.
    pub fn new<S: Into<EcoString>>(ctx: eval::Context<T>, input: S) -> Result<Self, Error> {
        let input = input.into();

        let (all, expr) = input
            .strip_prefix("all:")
            .map(|rest| (true, rest))
            .unwrap_or((false, &input));

        let set = ast::parse(expr)?.eval(&ctx).and_then(Value::expect_type)?;

        Ok(Self {
            input,
            all,
            ctx,
            set,
        })
    }
}

impl<T> ExpressionFilter<T> {
    /// The input expression the inner test set was parsed from.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Whether this test set expression has the special `all:` modifier.
    ///
    /// Handling this is up to the caller and has no impact on the inner test
    /// set.
    pub fn all(&self) -> bool {
        self.all
    }

    /// The context used to evaluate the inner test set.
    pub fn ctx(&self) -> &eval::Context<T> {
        &self.ctx
    }

    /// The inner test set.
    pub fn set(&self) -> &eval::Set<T> {
        &self.set
    }
}

impl<T> ExpressionFilter<T> {
    /// Applies a function to the inner test set, this is useful for
    /// optimization or for adding implicit test sets like wrapping a test set
    /// in `(...) ~ skip()`.
    pub fn map<F>(self, f: F) -> Self
    where
        F: FnOnce(eval::Set<T>) -> eval::Set<T>,
    {
        Self {
            set: f(self.set),
            ..self
        }
    }
}

impl<T> ExpressionFilter<T> {
    /// Whether the given test is contained in this test set. Note that this
    /// means a return value of `true` should _not_ be filtered out, but
    /// included in the set of tests to operate on.
    pub fn contains(&self, test: &T) -> Result<bool, eval::Error> {
        self.set.contains(&self.ctx, test)
    }
}

/// Returned by [`ExpressionFilter::new`].
#[derive(Debug, Error)]
pub enum Error {
    /// A error occurred during parsing.
    #[error(transparent)]
    Parse(#[from] ast::Error),

    /// An error occurred during evaluation.
    #[error(transparent)]
    Eval(#[from] eval::Error),
}
