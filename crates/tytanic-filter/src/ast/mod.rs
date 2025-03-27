//! Test set AST types.

use std::char::CharTryFromError;
use std::sync::LazyLock;

use pest::iterators::Pair;
use pest::pratt_parser::PrattParser;
use pest::Parser;
use thiserror::Error;
use tytanic_utils::fmt::Separators;

mod atom;
mod expr;
mod func;
mod glob;
mod id;
mod num;
mod parser;
mod pat;
mod regex;
mod str;

// This is an internal re-export and should _never_ leak outside this module.
use parser::Rule;

pub use self::atom::Atom;
pub use self::expr::{Expr, InfixOp, PrefixOp};
pub use self::func::Func;
pub use self::glob::Glob;
pub use self::id::Id;
pub use self::num::Num;
pub use self::pat::Pat;
pub use self::regex::Regex;
pub use self::str::Str;

/// The pratt parser defining the operator precedence.
pub(super) static PRATT_PARSER: LazyLock<PrattParser<Rule>> = LazyLock::new(|| {
    use pest::pratt_parser::{Assoc, Op};

    PrattParser::new()
        .op(Op::infix(Rule::infix_op_pipe, Assoc::Left) | Op::infix(Rule::infix_op_or, Assoc::Left))
        .op(Op::infix(Rule::infix_op_amper, Assoc::Left)
            | Op::infix(Rule::infix_op_and, Assoc::Left))
        .op(Op::infix(Rule::infix_op_tilde, Assoc::Left)
            | Op::infix(Rule::infix_op_diff, Assoc::Left))
        .op(Op::infix(Rule::infix_op_caret, Assoc::Left)
            | Op::infix(Rule::infix_op_xor, Assoc::Left))
        .op(Op::prefix(Rule::prefix_op_excl) | Op::prefix(Rule::prefix_op_not))
});

/// Parse the given input into a test set expression.
#[tracing::instrument(ret)]
pub fn parse(input: &str) -> Result<Expr, Error> {
    // unwrap main into its root level expr, removing the EOI pair
    let root_expr = parser::ExpressionParser::parse(Rule::main, input)
        .map_err(|err| {
            Box::new(err.renamed_rules(|r| r.token().to_owned()))
                as Box<dyn std::error::Error + Send + Sync + 'static>
        })?
        .next()
        .unwrap()
        .into_inner()
        .next()
        .unwrap();

    Expr::parse(root_expr, &PRATT_PARSER)
}

/// An error for parsing failures.
#[derive(Debug, Error)]
pub enum Error {
    /// The input ended unexpectedly.
    #[error(
        "expected one of {}, found nothing",
        Separators::comma_or().with(expected),
    )]
    UnexpectedEOI {
        /// The expected rules.
        expected: Vec<&'static str>,
    },

    /// Expected no further input, but found some.
    #[error("expected no further pairs, found {found}")]
    ExpectedEOI {
        /// The rule that was found.
        found: &'static str,
    },

    /// Expected a certain set of rules. but found a different rule.
    #[error(
        "expected one of {}, found {found}",
        Separators::comma_or().with(expected),
    )]
    UnexpectedRules {
        /// The expected rules
        expected: Vec<&'static str>,

        /// The rule that was found.
        found: &'static str,
    },

    /// A string escape did not describe a valid unicode code point.
    #[error("a string escape did not describe a valid unicode code point")]
    UnicodeEscape(#[from] CharTryFromError),

    /// A regex pattern could not be parsed.
    #[error("a regex pattern could not be parsed")]
    Regex(#[from] ::regex::Error),

    /// A glob pattern could not be parsed.
    #[error("a glob pattern could not be parsed")]
    Glob(#[from] ::glob::PatternError),

    /// Some other error occured.
    #[error("the expression could not be parsed")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),
}

/// An extension trait for pest iterators and its adapters.
pub trait PairsExt<'a> {
    /// If there is another pair ensure it is of the expected rules.
    fn try_expect_pair(&mut self, rules: &[Rule]) -> Result<Option<Pair<'a, Rule>>, Error>;

    /// Ensure there is a pair of one of the expected rules.
    fn expect_pair(&mut self, rules: &[Rule]) -> Result<Pair<'a, Rule>, Error>;

    /// Ensure there are no further pairs.
    fn expect_end(&mut self) -> Result<(), Error>;
}

impl<'a, I> PairsExt<'a> for I
where
    I: Iterator<Item = Pair<'a, Rule>>,
{
    fn try_expect_pair(&mut self, rules: &[Rule]) -> Result<Option<Pair<'a, Rule>>, Error> {
        self.next()
            .map(|pair| pair.expect_rules(rules).map(|_| pair))
            .transpose()
    }

    fn expect_pair(&mut self, rules: &[Rule]) -> Result<Pair<'a, Rule>, Error> {
        self.next()
            .ok_or_else(|| Error::UnexpectedEOI {
                expected: rules.iter().map(|r| r.name()).collect(),
            })
            .and_then(|pair| pair.expect_rules(rules).map(|_| pair))
    }

    fn expect_end(&mut self) -> Result<(), Error> {
        if let Some(pair) = self.next() {
            return Err(Error::ExpectedEOI {
                found: pair.as_rule().name(),
            });
        }

        Ok(())
    }
}

/// An extension trait for the [`Pair`] type.
pub trait PairExt<'a> {
    fn expect_rules(&self, rule: &[Rule]) -> Result<(), Error>;
}

impl<'a> PairExt<'a> for Pair<'a, Rule> {
    fn expect_rules(&self, rules: &[Rule]) -> Result<(), Error> {
        if !rules.contains(&self.as_rule()) {
            return Err(Error::UnexpectedRules {
                expected: rules.iter().map(|r| r.name()).collect(),
                found: self.as_rule().name(),
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ecow::eco_vec;

    use super::*;

    // TODO(tinger): test failures

    #[test]
    fn test_parse_single_string() {
        assert_eq!(
            parse(r#"'a string \'"#).unwrap(),
            Expr::Atom(Atom::Str(r#"a string \"#.into()))
        );
    }

    #[test]
    fn test_parse_double_string() {
        assert_eq!(
            parse(r#""a string \" \u{30}""#).unwrap(),
            Expr::Atom(Atom::Str(r#"a string " 0"#.into()))
        );
    }

    #[test]
    fn test_parse_identifier() {
        assert_eq!(
            parse("abc").unwrap(),
            Expr::Atom(Atom::Id(Id("abc".into())))
        );
        assert_eq!(
            parse("a-bc").unwrap(),
            Expr::Atom(Atom::Id(Id("a-bc".into())))
        );
        assert_eq!(
            parse("a__bc-").unwrap(),
            Expr::Atom(Atom::Id(Id("a__bc-".into())))
        );
    }

    #[test]
    fn test_parse_number() {
        assert_eq!(parse("1234").unwrap(), Expr::Atom(Atom::Num(1234.into())));
        assert_eq!(parse("1_000").unwrap(), Expr::Atom(Atom::Num(1000.into())));
    }

    #[test]
    fn test_parse_pattern_string() {
        assert_eq!(
            parse("r:'^abc*$'").unwrap(),
            Expr::Atom(Atom::Pat(Pat::Regex(Regex::new("^abc*$").unwrap())))
        );
        assert_eq!(
            parse(r#"glob:"a/**/b""#).unwrap(),
            Expr::Atom(Atom::Pat(Pat::Glob(Glob::new("a/**/b").unwrap())))
        );
    }

    #[test]
    fn test_parse_pattern_raw() {
        assert_eq!(
            parse("g:a/**/b").unwrap(),
            Expr::Atom(Atom::Pat(Pat::Glob(Glob::new("a/**/b").unwrap())))
        );
        assert_eq!(
            parse("e:a/b").unwrap(),
            Expr::Atom(Atom::Pat(Pat::Exact("a/b".into())))
        );
        assert_eq!(
            parse("r:(a/b\\))").unwrap(),
            Expr::Atom(Atom::Pat(Pat::Regex(Regex::new("(a/b\\))").unwrap())))
        );
        assert_eq!(
            parse("r:(a/b{3,4})").unwrap(),
            Expr::Atom(Atom::Pat(Pat::Regex(Regex::new("(a/b{3,4})").unwrap())))
        );
    }

    #[test]
    fn test_parse_pattern_raw_termination() {
        assert_eq!(
            parse("foo(e:bar)").unwrap(),
            Expr::Func(Func {
                id: Id("foo".into()),
                args: eco_vec![Expr::Atom(Atom::Pat(Pat::Exact("bar".into())))]
            }),
        );
        assert_eq!(
            parse("foo(e:bar, r:qux(quuz))").unwrap(),
            Expr::Func(Func {
                id: Id("foo".into()),
                args: eco_vec![
                    Expr::Atom(Atom::Pat(Pat::Exact("bar".into()))),
                    Expr::Atom(Atom::Pat(Pat::Regex(Regex::new("qux(quuz)").unwrap())))
                ]
            }),
        );
        assert_eq!(
            parse("foo(e:bar, r:qux(quuz{3,4}))").unwrap(),
            Expr::Func(Func {
                id: Id("foo".into()),
                args: eco_vec![
                    Expr::Atom(Atom::Pat(Pat::Exact("bar".into()))),
                    Expr::Atom(Atom::Pat(Pat::Regex(Regex::new("qux(quuz{3,4})").unwrap())))
                ]
            }),
        );
    }

    #[test]
    fn test_parse_pattern_in_args() {
        assert_eq!(
            parse("func(e:foo)").unwrap(),
            Expr::Func(Func {
                id: Id("func".into()),
                args: eco_vec![Expr::Atom(Atom::Pat(Pat::Exact(Str("foo".into()))))],
            })
        );
        assert_eq!(
            parse("func(e:foo, bar)").unwrap(),
            Expr::Func(Func {
                id: Id("func".into()),
                args: eco_vec![
                    Expr::Atom(Atom::Pat(Pat::Exact(Str("foo".into())))),
                    Expr::Atom(Atom::Id(Id("bar".into()))),
                ],
            })
        );
    }

    #[test]
    fn test_parse_func_no_args() {
        assert_eq!(
            parse("func()").unwrap(),
            Expr::Func(Func {
                id: Id("func".into()),
                args: eco_vec![],
            })
        );
        assert_eq!(
            parse("func(  )").unwrap(),
            Expr::Func(Func {
                id: Id("func".into()),
                args: eco_vec![],
            })
        );
    }

    #[test]
    fn test_parse_func_simple_args() {
        assert_eq!(
            parse("func( 1  , e:'a/b')").unwrap(),
            Expr::Func(Func {
                id: Id("func".into()),
                args: eco_vec![
                    Expr::Atom(Atom::Num(1.into())),
                    Expr::Atom(Atom::Pat(Pat::Exact("a/b".into())))
                ],
            })
        );
    }

    #[test]
    fn test_parse_prefix_expression() {
        assert_eq!(
            parse("! not 0").unwrap(),
            Expr::Prefix {
                op: PrefixOp::Not,
                expr: Arc::new(Expr::Prefix {
                    op: PrefixOp::Not,
                    expr: Arc::new(Expr::Atom(Atom::Num(Num(0)))),
                }),
            }
        );
    }

    #[test]
    fn test_parse_infix_expression() {
        assert_eq!(
            parse("0 and 1 or 2").unwrap(),
            Expr::Infix {
                op: InfixOp::Union,
                lhs: Arc::new(Expr::Infix {
                    op: InfixOp::Inter,
                    lhs: Arc::new(Expr::Atom(Atom::Num(Num(0)))),
                    rhs: Arc::new(Expr::Atom(Atom::Num(Num(1)))),
                }),
                rhs: Arc::new(Expr::Atom(Atom::Num(Num(2)))),
            }
        );

        assert_eq!(
            parse("0 and (1 or 2)").unwrap(),
            Expr::Infix {
                op: InfixOp::Inter,
                lhs: Arc::new(Expr::Atom(Atom::Num(Num(0)))),
                rhs: Arc::new(Expr::Infix {
                    op: InfixOp::Union,
                    lhs: Arc::new(Expr::Atom(Atom::Num(Num(1)))),
                    rhs: Arc::new(Expr::Atom(Atom::Num(Num(2)))),
                }),
            }
        );
    }

    #[test]
    fn test_parse_expression() {
        assert_eq!(
            parse("regex:'abc' and not (4_2 | func(0))").unwrap(),
            Expr::Infix {
                op: InfixOp::Inter,
                lhs: Arc::new(Expr::Atom(Atom::Pat(Pat::Regex(
                    Regex::new("abc").unwrap()
                )))),
                rhs: Arc::new(Expr::Prefix {
                    op: PrefixOp::Not,
                    expr: Arc::new(Expr::Infix {
                        op: InfixOp::Union,
                        lhs: Arc::new(Expr::Atom(Atom::Num(Num(42)))),
                        rhs: Arc::new(Expr::Func(Func {
                            id: Id("func".into()),
                            args: eco_vec![Expr::Atom(Atom::Num(Num(0)))]
                        })),
                    }),
                }),
            }
        );
    }
}
