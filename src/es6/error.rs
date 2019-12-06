use std::fmt;

use esparse;
use esparse::lex::{self};
use esparse::skip::{self};

#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub span: esparse::ast::SpanT<String, esparse::ast::Loc>,
}

#[derive(Debug)]
pub enum ErrorKind {
    Expected(&'static str),
    ParseStrLitError(lex::ParseStrLitError),
}

impl From<skip::Error> for Error {
    fn from(inner: skip::Error) -> Self {
        Error {
            kind: match inner.kind {
                skip::ErrorKind::Expected(s) => ErrorKind::Expected(s),
            },
            span: inner.span,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} at {}", self.kind, self.span,)
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ErrorKind::Expected(s) => write!(f, "expected {}", s),
            ErrorKind::ParseStrLitError(ref error) => {
                write!(f, "invalid string literal: {}", error)
            }
        }
    }
}
