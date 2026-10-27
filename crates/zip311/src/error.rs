use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error("not a payment disclosure: {0}")]
    Text(String),
    #[error("unknown human-readable part `{0}` (expected zpd or zpdtest)")]
    Hrp(String),
    #[error("unsupported disclosure version {0}")]
    Version(u8),
    #[error("unknown network byte {0}")]
    Network(u8),
    #[error("disclosure ends early")]
    Truncated,
    #[error("non-canonical length prefix")]
    NonCanonical,
    #[error("{0} exceeds the limit of {1}")]
    TooLarge(&'static str, usize),
    #[error("{0} trailing bytes after the disclosure")]
    Trailing(usize),
    #[error("text says {text} but the encoded disclosure says {inner}")]
    NetworkMismatch { text: String, inner: String },
}

#[derive(Debug, Error)]
pub enum ProveError {
    #[error("the transaction has no Ironwood bundle")]
    NoIronwoodBundle,
    #[error("at least one spent note is needed to prove spend authority")]
    NoSpends,
    #[error("note #{0} was not spent by this transaction (its nullifier is not among the transaction's actions)")]
    NoteNotSpentHere(usize),
    #[error("the Merkle path for note #{0} does not lead to the transaction's anchor")]
    WrongAnchor(usize),
    #[error("output action {0} is out of range")]
    OutputOutOfRange(u32),
    #[error("output action {0} was not created with this wallet's outgoing viewing key")]
    NotOurOutput(u32),
    #[error("the transaction pays no one outside this wallet that can be opened with its outgoing viewing key")]
    NoPayments,
    #[error("challenge is longer than {0} bytes")]
    MessageTooLong(usize),
    #[error("could not build the disclosure bundle: {0}")]
    Build(String),
    #[error("could not serialize the transaction: {0}")]
    Io(#[from] std::io::Error),
}
