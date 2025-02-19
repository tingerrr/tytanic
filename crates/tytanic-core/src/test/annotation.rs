//! Test annotations are used to override settings of a test.
//!
//! Annotations may be placed on a leading doc comment block (indicated by
//! `///`), such a doc comment block can be placed after initial empty or
//! regular comment lines, but must come before any content. All annotations in
//! such a block must be at the start, once non-annotation content is
//! encountered parsing stops.
//!
//! ```typst
//! // SPDX-License-Identifier: MIT
//!
//! /// [skip]
//! /// [max-delta: 10]
//! ///
//! /// Synopsis:
//! /// ...
//!
//! #set page("a4")
//! ...
//! ```

use std::str::FromStr;

use ecow::{EcoString, EcoVec};
use thiserror::Error;

use crate::config::Direction;

/// An error which may occur while parsing an annotation.
#[derive(Debug, Error)]
pub enum ParseAnnotationError {
    /// The delimiter were missing or unclosed.
    #[error("the annotation had only one or no delimiter")]
    MissingDelimiter,

    /// The annotation identifier is unknown, invalid or empty.
    #[error("unknown or invalid annotation identifier: {0:?}")]
    Unknown(EcoString),

    /// The annotation expected no argument, but received one.
    #[error("the annotation {0} expected no argument, but received one")]
    UnexpectedArg(&'static str),

    /// The annotation expected an argument, but received none.
    #[error("the annotation {0} expected an argument, but received none")]
    MissingArg(&'static str),

    /// An error occured while parsing the annotation.
    #[error("an error occured while parsing the annotation")]
    Other(#[source] Box<dyn std::error::Error + Sync + Send + 'static>),
}

/// A test annotation used to configure test specific behavior.
///
/// Test annotations are placed on doc comments at the top of a test's source
/// file:
///
/// Each annotation is on its own line.
#[derive(Debug, Clone, PartialEq)]
pub enum Annotation {
    /// The skip annotation, this adds a test to the built in `skip` test set.
    Skip,

    /// The template annotation, this gives a test access to template assets,
    /// but disables the augmented standard library.
    Template,

    /// The direction to use for diffing the documents.
    Dir(Direction),

    /// The pixel per inch to use for exporting the documents.
    Ppi(f32),

    /// The maximum allowed per pixel delta to use for comparsion.
    MaxDelta(u8),

    /// The maximum allowed amount of deviations to use fro comparison.
    MaxDeviations(usize),
}

impl Annotation {
    /// Collects all annotations found within a test's source code.
    pub fn collect(source: &str) -> Result<EcoVec<Self>, ParseAnnotationError> {
        // skip regular comments and leading empty lines
        let lines = source.lines().skip_while(|line| {
            line.strip_prefix("//")
                .is_some_and(|rest| !rest.starts_with('/'))
                || line.trim().is_empty()
        });

        // then collect all consecutive doc comment lines
        let lines = lines.map_while(|line| line.strip_prefix("///").map(str::trim));

        // ignore empty ones
        let lines = lines.filter(|line| !line.is_empty());

        // take only those which start with an annotation deimiter
        let lines = lines.take_while(|line| line.starts_with('['));

        lines.map(str::parse).collect()
    }
}

impl FromStr for Annotation {
    type Err = ParseAnnotationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some(rest) = s.strip_prefix('[') else {
            return Err(ParseAnnotationError::MissingDelimiter);
        };

        let Some(rest) = rest.strip_suffix(']') else {
            return Err(ParseAnnotationError::MissingDelimiter);
        };

        let (id, arg) = match rest.split_once(':') {
            Some((id, arg)) => (id, Some(arg.trim())),
            None => (rest, None),
        };

        match id.trim() {
            "skip" => {
                if arg.is_some() {
                    Err(ParseAnnotationError::UnexpectedArg("skip"))
                } else {
                    Ok(Annotation::Skip)
                }
            }
            "template" => {
                if arg.is_some() {
                    Err(ParseAnnotationError::UnexpectedArg("template"))
                } else {
                    Ok(Annotation::Skip)
                }
            }
            "dir" => match arg {
                Some(arg) => match arg.trim() {
                    "ltr" => Ok(Annotation::Dir(Direction::Ltr)),
                    "rtl" => Ok(Annotation::Dir(Direction::Rtl)),
                    _ => Err(ParseAnnotationError::Other(
                        format!("invalid direction {arg:?}, expected one of ltr or rtl").into(),
                    )),
                },
                None => Err(ParseAnnotationError::MissingArg("dir")),
            },
            "ppi" => match arg {
                Some(arg) => match arg.trim().parse() {
                    Ok(arg) => Ok(Annotation::Ppi(arg)),
                    Err(err) => Err(ParseAnnotationError::Other(err.into())),
                },
                None => Err(ParseAnnotationError::MissingArg("ppi")),
            },
            "max-delta" => match arg {
                Some(arg) => match arg.trim().parse() {
                    Ok(arg) => Ok(Annotation::MaxDelta(arg)),
                    Err(err) => Err(ParseAnnotationError::Other(err.into())),
                },
                None => Err(ParseAnnotationError::MissingArg("max-delta")),
            },
            "max-deviations" => match arg {
                Some(arg) => match arg.trim().parse() {
                    Ok(arg) => Ok(Annotation::MaxDeviations(arg)),
                    Err(err) => Err(ParseAnnotationError::Other(err.into())),
                },
                None => Err(ParseAnnotationError::MissingArg("max-deviations")),
            },
            _ => Err(ParseAnnotationError::Unknown(id.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_annotation_from_str() {
        assert_eq!(Annotation::from_str("[skip]").unwrap(), Annotation::Skip);
        assert_eq!(Annotation::from_str("[ skip  ]").unwrap(), Annotation::Skip);

        assert!(Annotation::from_str("[ skip  ").is_err());
        assert!(Annotation::from_str("[unknown]").is_err());
    }

    #[test]
    fn test_annotation_unexpected_arg() {
        assert!(Annotation::from_str("[skip:]").is_err());
        assert!(Annotation::from_str("[skip: 10]").is_err());
    }

    #[test]
    fn test_annotation_expected_arg() {
        assert!(Annotation::from_str("[ppi]").is_err());
        assert!(Annotation::from_str("[max-delta:]").is_err());
    }

    #[test]
    fn test_annotation_arg() {
        assert_eq!(
            Annotation::from_str("[max-deviations: 20]").unwrap(),
            Annotation::MaxDeviations(20)
        );
        assert_eq!(
            Annotation::from_str("[ppi: 42.5]").unwrap(),
            Annotation::Ppi(42.5)
        );
    }

    #[test]
    fn test_collect_book_example() {
        let source = "\
        /// [skip]    \n\
        ///           \n\
        /// Synopsis: \n\
        /// ...       \n\
                      \n\
        #import \"/src/internal.typ\": foo \n\
        ...";

        assert_eq!(Annotation::collect(source).unwrap(), [Annotation::Skip]);
    }

    #[test]
    fn test_collect_issue_109() {
        assert_eq!(
            Annotation::collect("///[skip]").unwrap(),
            [Annotation::Skip]
        );
        assert_eq!(Annotation::collect("///").unwrap(), []);
        assert_eq!(
            Annotation::collect("/// [skip]").unwrap(),
            [Annotation::Skip]
        );
        assert_eq!(
            Annotation::collect("///[skip]\n///").unwrap(),
            [Annotation::Skip]
        );
    }
}
