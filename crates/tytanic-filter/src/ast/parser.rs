use std::fmt::Debug;
use std::hash::Hash;

use pest_derive::Parser;

/// A parser for test set expressions.
#[derive(Parser)]
#[grammar = "ast/grammar.pest"]
pub(super) struct ExpressionParser;

use super::{InfixOp, PrefixOp};

impl Rule {
    /// Turns this rule into the respective prefix operator.
    pub fn to_prefix(self) -> Option<PrefixOp> {
        Some(match self {
            Rule::prefix_op_excl | Rule::prefix_op_not => PrefixOp::Not,
            _ => return None,
        })
    }

    /// Turns this rule into the respective infix operator.
    pub fn to_infix(self) -> Option<InfixOp> {
        Some(match self {
            Rule::infix_op_pipe | Rule::infix_op_or => InfixOp::Union,
            Rule::infix_op_amper | Rule::infix_op_and => InfixOp::Inter,
            Rule::infix_op_tilde | Rule::infix_op_diff => InfixOp::Diff,
            Rule::infix_op_caret | Rule::infix_op_xor => InfixOp::SymDiff,
            _ => return None,
        })
    }

    /// The token this rule corresponds to, or a sensble substitute for
    /// diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            Rule::EOI => "EOI",
            Rule::main | Rule::expr | Rule::expr_term | Rule::expr_atom => "expression",
            Rule::expr_group => "expression group",
            Rule::prefix_op => "prefix op",
            Rule::prefix_op_excl => "symbol complement op",
            Rule::prefix_op_not => "literal complement op",
            Rule::infix_op => "infix op",
            Rule::infix_op_caret => "symbol symmetric difference op",
            Rule::infix_op_amper => "symbol intersection op",
            Rule::infix_op_tilde => "symbol difference op",
            Rule::infix_op_pipe => "symbol union op",
            Rule::infix_op_xor => "literal symmetric difference op",
            Rule::infix_op_and => "literal intersection op",
            Rule::infix_op_diff => "literal difference op",
            Rule::infix_op_or => "literal union op",
            Rule::id => "identifier",
            Rule::func | Rule::func_args | Rule::func_args_inner => "function arguments",
            Rule::func_args_sep => "comma",
            Rule::func_args_delim_open => "opening parenthesis",
            Rule::func_args_delim_close => "closing parenthesis",
            Rule::pat => "pattern",
            Rule::pat_kind => "pattern kind",
            Rule::pat_kind_glob => "glob pattern kind",
            Rule::pat_kind_regex => "regex pattern kind",
            Rule::pat_kind_exact => "exact pattern kind",
            Rule::pat_inner | Rule::pat_pat => "pattern",
            Rule::pat_raw_lit
            | Rule::pat_raw_term_outer
            | Rule::pat_raw_term_inner
            | Rule::pat_raw_char_esc => "raw pattern literal",
            Rule::pat_raw_char_outer
            | Rule::pat_raw_char_inner
            | Rule::pat_raw_char_normal_outer
            | Rule::pat_raw_char_normal_inner => "any",
            Rule::pat_sep => "colon",
            Rule::str => "string",
            Rule::str_single | Rule::str_single_inner => "single quoted string",
            Rule::str_double | Rule::str_double_inner => "double quoted string",
            Rule::str_single_delim => "single quote",
            Rule::str_double_delim => "double quote",
            Rule::str_single_char | Rule::str_double_char => "any",
            Rule::str_double_esc_meta
            | Rule::str_double_esc_ascii
            | Rule::str_double_esc_unicode => "escape",
            Rule::num | Rule::num_inner => "number",
            Rule::num_part => "digit",
            Rule::num_sep => "underscore",
            Rule::WHITESPACE => "whitespace",
        }
    }

    /// The token for this rule to use in diagnostics.
    pub fn token(self) -> &'static str {
        match self {
            Rule::EOI => "<EOI>",
            Rule::main | Rule::expr | Rule::expr_term | Rule::expr_atom => "<expr>",
            Rule::expr_group => "(...)",
            Rule::prefix_op => "<prefix op>",
            Rule::prefix_op_excl => "!",
            Rule::prefix_op_not => "not",
            Rule::infix_op => "<infix op>",
            Rule::infix_op_caret => "^",
            Rule::infix_op_amper => "&",
            Rule::infix_op_tilde => "~",
            Rule::infix_op_pipe => "|",
            Rule::infix_op_xor => "xor",
            Rule::infix_op_and => "and",
            Rule::infix_op_diff => "diff",
            Rule::infix_op_or => "or",
            Rule::id => "<ident>",
            Rule::func | Rule::func_args | Rule::func_args_inner => "<args>",
            Rule::func_args_sep => "<comma>",
            Rule::func_args_delim_open => "(",
            Rule::func_args_delim_close => ")",
            Rule::pat => "<kind>:<pattern>",
            Rule::pat_kind => "<pattern kind>",
            Rule::pat_kind_glob => "glob",
            Rule::pat_kind_regex => "regex",
            Rule::pat_kind_exact => "exact",
            Rule::pat_inner | Rule::pat_pat => "<pattern>",
            Rule::pat_raw_lit
            | Rule::pat_raw_term_outer
            | Rule::pat_raw_term_inner
            | Rule::pat_raw_char_esc => "<raw pattern>",
            Rule::pat_raw_char_outer
            | Rule::pat_raw_char_inner
            | Rule::pat_raw_char_normal_outer
            | Rule::pat_raw_char_normal_inner => "<ANY>",
            Rule::pat_sep => ":",
            Rule::str => "<str>",
            Rule::str_single | Rule::str_single_inner => "'...'",
            Rule::str_double | Rule::str_double_inner => "\"...\"",
            Rule::str_single_delim => "'",
            Rule::str_double_delim => "\"",
            Rule::str_single_char | Rule::str_double_char => "<ANY>",
            Rule::str_double_esc_meta
            | Rule::str_double_esc_ascii
            | Rule::str_double_esc_unicode => "<escape>",
            Rule::num | Rule::num_inner => "<number>",
            Rule::num_part => "<digit>",
            Rule::num_sep => "_",
            Rule::WHITESPACE => "<WHITESPACE>",
        }
    }
}
