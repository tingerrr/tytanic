//! Test set evaluation.

use std::collections::BTreeMap;
use std::fmt::{Debug, Display};

use ecow::EcoVec;
use thiserror::Error;
use tytanic_utils::fmt::{Separators, Term};

use super::ast::Id;

mod func;
mod set;
mod value;

pub use self::func::Func;
pub use self::set::Set;
pub use self::value::{TryFromValue, Type, Value};

/// A marker trait for tests, this is automatically implemented for all clonable
/// static types.
pub trait Test: Clone + 'static {
    /// The id of a test, this is used for matching on tests using test sets
    /// created from pattern literals.
    fn id(&self) -> &str;
}

/// A trait for expressions to be evaluated and matched.
pub trait Eval<T: Test> {
    /// Evaluates this expression to a value.
    fn eval(&self, ctx: &Context<T>) -> Result<Value<T>, Error>;
}

/// An evaluation context used to retrieve bindings in test set expressions.
#[derive(Debug, Clone)]
pub struct Context<T> {
    /// The bindings available for evaluation.
    bindings: BTreeMap<Id, Value<T>>,
}

impl<T> Context<T> {
    /// Create a new evaluation context with no bindings.
    pub fn new() -> Self {
        Self {
            bindings: BTreeMap::new(),
        }
    }
}

impl<T> Context<T> {
    /// Inserts a new binding, possibly overriding an old one, returns the old
    /// binding if there was one.
    pub fn bind<V: Into<Value<T>>>(&mut self, id: Id, value: V) -> Option<Value<T>> {
        tracing::trace!(id = %id.as_str(), "binding value into eval context");
        self.bindings.insert(id, value.into())
    }

    /// Resolves a binding with the given identifier.
    pub fn resolve<I: AsRef<str>>(&self, id: I) -> Result<Value<T>, Error>
    where
        T: Clone,
    {
        tracing::trace!(id = %id.as_ref(), "resolving value from eval context");
        let id = id.as_ref();
        self.bindings
            .get(id)
            .cloned()
            .ok_or_else(|| Error::UnknownBinding { id: id.into() })
    }

    /// Find similar bindings to the given identifier.
    pub fn find_similar(&self, id: &str) -> Vec<Id> {
        self.bindings
            .keys()
            .filter(|cand| strsim::jaro(id, cand.as_str()) > 0.7)
            .cloned()
            .collect()
    }
}

impl<T> Default for Context<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// An error that occurs when a test set expression is evaluated.
#[derive(Debug, Error)]
pub enum Error {
    /// The requested binding could not be found.
    UnknownBinding {
        /// The given identifier.
        id: String,
    },

    /// A function received an incorrect argument count.
    InvalidArgumentCount {
        /// The identifier of the function.
        func: String,

        /// The minimum or exact expected number of arguments, interpretation
        /// depends on `is_min`.
        expected: usize,

        /// Whether the expected number is the minimum and allows more arguments.
        is_min: bool,

        /// The number of arguments passed.
        found: usize,
    },

    /// An invalid type was used in an expression.
    TypeMismatch {
        /// The expected types.
        expected: EcoVec<Type>,

        /// The given type.
        found: Type,
    },

    /// A custom error type.
    Custom(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::UnknownBinding { id } => write!(f, "unknown binding: {id}"),
            Error::InvalidArgumentCount {
                func,
                expected,
                is_min,
                found,
            } => {
                let (found, ex) = (*found, *expected);

                if ex == 0 {
                    write!(
                        f,
                        "function {func} expects no {}, got {}",
                        Term::simple("argument").with(ex),
                        found,
                    )?;
                } else if *is_min {
                    write!(
                        f,
                        "function {func} expects at least {ex} {}, got {}",
                        Term::simple("argument").with(ex),
                        found,
                    )?;
                } else {
                    write!(
                        f,
                        "function {func} expects exactly {ex} {}, got {}",
                        Term::simple("argument").with(ex),
                        found,
                    )?;
                }

                Ok(())
            }
            Error::TypeMismatch { expected, found } => write!(
                f,
                "expected {}, found <{}>",
                Separators::comma_or().with(expected.iter().map(|t| format!("<{}>", t.name()))),
                found.name(),
            ),
            Error::Custom(err) => write!(f, "{err}"),
        }
    }
}

/// Ensure Context<T> is thread safe if T is.
#[allow(dead_code)]
fn assert_traits() {
    tytanic_utils::assert::send::<Context<()>>();
    tytanic_utils::assert::sync::<Context<()>>();
}
