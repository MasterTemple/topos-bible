/// Convenience for binaries and tests that mix error types
pub type AnyResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Errors from parsing references and building data or filters
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ToposError {
    #[error("expected the format `{0}`")]
    ExpectedFormat(&'static str),
    #[error("no segments found")]
    NoSegments,
    #[error("expected exactly 1 segment, found {0}")]
    MultipleSegments(usize),
    #[error("cannot turn a {from} into a {to}")]
    Coerce {
        from: &'static str,
        to: &'static str,
    },
    #[error("could not parse the passage {0:?}")]
    InvalidPassage(String),
    #[error("unknown book {0:?}")]
    UnknownBook(String),
    #[error("unknown genre {0:?}")]
    UnknownGenre(String),
    #[error("unknown testament {0:?} (expected `old` or `new`)")]
    UnknownTestament(String),
    #[error("invalid OSIS reference {0:?}")]
    InvalidOsis(String),
    #[error("invalid book data: {0}")]
    InvalidData(String),
}

pub type ToposResult<T> = Result<T, ToposError>;
