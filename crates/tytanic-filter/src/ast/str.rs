use std::fmt::Debug;
use std::ops::Deref;

use ecow::{eco_vec, EcoString};
use pest::iterators::Pair;

use super::{Error, PairExt, PairsExt, Rule};
use crate::eval::{self, Context, Eval, Test, TryFromValue, Type, Value};

/// A string literal node.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Str(pub EcoString);

impl Str {
    /// The inner string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Unwraps the inner eco string.
    pub fn into_inner(self) -> EcoString {
        self.0
    }
}

impl Debug for Str {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for Str {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl AsRef<str> for Str {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<EcoString> for Str {
    fn from(value: EcoString) -> Self {
        Self(value)
    }
}

impl From<String> for Str {
    fn from(value: String) -> Self {
        Self(value.into())
    }
}

impl From<&str> for Str {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

impl From<Str> for EcoString {
    fn from(value: Str) -> Self {
        value.into_inner()
    }
}

impl<T: Test> Eval<T> for Str {
    fn eval(&self, _ctx: &Context<T>) -> Result<Value<T>, eval::Error> {
        Ok(Value::Str(self.clone()))
    }
}

impl<T> TryFromValue<T> for Str {
    fn try_from_value(value: Value<T>) -> Result<Self, eval::Error> {
        Ok(match value {
            Value::Str(str) => str,
            _ => {
                return Err(eval::Error::TypeMismatch {
                    expected: eco_vec![Type::Str],
                    found: value.as_type(),
                })
            }
        })
    }
}

impl Str {
    pub(super) fn parse(pair: Pair<'_, Rule>) -> Result<Self, Error> {
        pair.expect_rules(&[Rule::str_single, Rule::str_double])?;

        let mut pairs = pair.into_inner();
        let start = pairs.expect_pair(&[Rule::str_single_delim, Rule::str_double_delim])?;
        let inner = pairs.expect_pair(&[Rule::str_single_inner, Rule::str_double_inner])?;
        let _ = pairs.expect_pair(&[start.as_rule()])?;
        pairs.expect_end()?;

        match inner.as_rule() {
            Rule::str_single_inner => Ok(Self(inner.as_str().into())),
            Rule::str_double_inner => {
                if !inner.as_str().contains('\\') {
                    Ok(Self(inner.as_str().into()))
                } else {
                    let mut buf = String::with_capacity(inner.as_str().len());

                    let mut rest = inner.as_str();
                    while let Some((lit, esc)) = rest.split_once('\\') {
                        buf.push_str(lit);

                        if esc.starts_with(['\\', '"', 'n', 'r', 't']) {
                            match esc.as_bytes()[0] {
                                b'\\' => buf.push('\\'),
                                b'"' => buf.push('"'),
                                b'n' => buf.push('\n'),
                                b'r' => buf.push('\r'),
                                b't' => buf.push('\t'),
                                _ => unreachable!(),
                            }
                            rest = &esc[1..];
                        } else if let Some(esc) = esc.strip_prefix("u{") {
                            let (digits, other) =
                                esc.split_once('}').expect("parser ensures closing '}'");

                            buf.push(
                                u32::from_str_radix(digits, 16)
                                    .expect("parser ensures hex digits only")
                                    .try_into()?,
                            );

                            rest = other;
                        } else {
                            unreachable!(
                                "unhandled string escape sequence: {:?}",
                                esc.split_once(' ').map(|(p, _)| p).unwrap_or(esc)
                            );
                        }
                    }

                    Ok(Self(buf.into()))
                }
            }
            _ => unreachable!(),
        }
    }
}
