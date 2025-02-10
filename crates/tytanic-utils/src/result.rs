//! Extensions for the [`Result`] type.

use std::io;

use crate::private::Sealed;

/// Extensions for the [`Result`] type.
#[allow(private_bounds)]
pub trait ResultEx<T, E>: Sealed {
    /// Ignores the subset of the error for which the `check` returns true,
    /// returning `None` instead.
    ///
    /// # Examples
    /// ```no_run
    /// # use std::fs;
    /// # use std::io::ErrorKind;
    /// use tytanic_utils::result::ResultEx;
    /// // if foo doesn't exist we get None
    /// // if another error is returned it is propagated
    /// assert_eq!(
    ///     fs::read_to_string("foo.txt").ignore(
    ///         |e| e.kind() == ErrorKind::NotFound,
    ///     )?,
    ///     Some(String::from("foo")),
    /// );
    /// assert_eq!(
    ///     fs::read_to_string("not-found.txt").ignore(
    ///         |e| e.kind() == ErrorKind::NotFound,
    ///     )?,
    ///     None,
    /// );
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    fn ignore<F>(self, check: F) -> Result<Option<T>, E>
    where
        F: FnOnce(&E) -> bool;

    /// Ignores the subset of the error for which the `check` returns true,
    /// returning `Default::default` instead.
    ///
    /// # Examples
    /// ```no_run
    /// # use std::fs;
    /// # use std::io::ErrorKind;
    /// use tytanic_utils::result::ResultEx;
    /// // if foo doesn't exist we get ""
    /// // if another error is returned it is propagated
    /// assert_eq!(
    ///     fs::read_to_string("foo.txt").ignore_default(
    ///         |e| e.kind() == ErrorKind::NotFound,
    ///     )?,
    ///     String::from("foo"),
    /// );
    /// assert_eq!(
    ///     fs::read_to_string("not-found.txt").ignore_default(
    ///         |e| e.kind() == ErrorKind::NotFound,
    ///     )?,
    ///     String::new(),
    /// );
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    fn ignore_default<F>(self, check: F) -> Result<T, E>
    where
        T: Default,
        F: FnOnce(&E) -> bool;

    /// Ignores the subset of the error for which the `check` returns true,
    /// returning the result of `value` instead.
    ///
    /// # Examples
    /// ```no_run
    /// # use std::fs;
    /// # use std::io::ErrorKind;
    /// use tytanic_utils::result::ResultEx;
    /// // if foo doesn't exist we get "foo"
    /// // if another error is returned it is propagated
    /// assert_eq!(
    ///     fs::read_to_string("foo.txt").ignore_with(
    ///         |e| e.kind() == ErrorKind::NotFound,
    ///         |_| String::from("foo"),
    ///     )?,
    ///     String::from("foo"),
    /// );
    /// assert_eq!(
    ///     fs::read_to_string("not-found.txt").ignore_with(
    ///         |e| e.kind() == ErrorKind::NotFound,
    ///         |_| String::from("foo"),
    ///     )?,
    ///     String::from("bar"),
    /// );
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    fn ignore_with<F, G>(self, check: F, value: G) -> Result<T, E>
    where
        F: FnOnce(&E) -> bool,
        G: FnOnce(&E) -> T;
}

impl<T, E> ResultEx<T, E> for Result<T, E> {
    fn ignore<F>(self, check: F) -> Result<Option<T>, E>
    where
        F: FnOnce(&E) -> bool,
    {
        self.map(Some).ignore_with(check, |_| None)
    }

    fn ignore_default<F>(self, check: F) -> Result<T, E>
    where
        T: Default,
        F: FnOnce(&E) -> bool,
    {
        self.ignore_with(check, |_| T::default())
    }

    fn ignore_with<F, G>(self, check: F, value: G) -> Result<T, E>
    where
        F: FnOnce(&E) -> bool,
        G: FnOnce(&E) -> T,
    {
        match self {
            Err(err) if check(&err) => Ok(value(&err)),
            x => x,
        }
    }
}

/// A check for [`ResultEx`] methods which ignores [`io::ErrorKind::NotFound`].
///
/// # Examples
/// ```no_run
/// # use std::fs;
/// # use std::io::ErrorKind;
/// use tytanic_utils::result::ResultEx;
/// use tytanic_utils::result::io_not_found;
/// assert_eq!(
///     fs::read_to_string("found.txt").ignore_default(io_not_found)?,
///     String::from("foo"),
/// );
/// assert_eq!(
///     fs::read_to_string("not-found.txt").ignore_default(io_not_found)?,
///     String::from(""),
/// );
/// # Ok::<_, Box<dyn std::error::Error>>(())
/// ```
pub fn io_not_found(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::NotFound
}
