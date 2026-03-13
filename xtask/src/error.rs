use std::fmt::{Display, Formatter};

pub type Result<T> = std::result::Result<T, XtaskError>;

#[derive(Debug)]
pub enum XtaskError {
    Io(std::io::Error),
    Message(String),
}

impl Display for XtaskError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Message(message) => write!(f, "{message}"),
        }
    }
}

impl From<std::io::Error> for XtaskError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
