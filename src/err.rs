use std::error::{self, FromError};
use std::fmt;
use std::io;

pub enum Kind {
    InvalidTypeAndFlavor,
    InvalidLength,
    NumberOverflow,
    IoError(io::Error),
}

pub struct Error {
    pub kind:   Kind,
    pub offset: usize,
    pub cause:  Option<Box<Error>>,
}

impl Error {
    pub fn new (kind: Kind, offset: usize, cause: Option<Box<Error>>) -> Error {
        Error {
            kind: kind,
            offset: offset,
            cause: cause,
        }
    }

    pub fn wrap (self, kind: Kind, offset: usize) -> Error {
        Error::new(kind, offset, Some(Box::new(self)))
    }
}

impl fmt::Display for Error {
    fn fmt (&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", *self)
    }
}

impl fmt::Debug for Error {
    fn fmt (&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "error at {}: {}", self.offset, error::Error::description(self))
    }
}

impl error::Error for Error {
    fn description (&self) -> &str {
        match self.kind {
            Kind::InvalidTypeAndFlavor  => "tag number and flavor mismatch",
            Kind::InvalidLength => "Indefinite length is only allowed for constructed tags",
            Kind::NumberOverflow => "BER number is larger than 8 bytes",
            Kind::IoError(ref x) => x.description(),
        }
    }

    fn cause (&self) -> Option<&error::Error> {
        match self.cause {
            Some(ref c) => Some(&**c),
            None => None,
        }
    }
}

impl FromError<io::Error> for Error {
    fn from_error (err: io::Error) -> Error {
        Error {
            kind: Kind::IoError(err),
            offset: 0,
            cause: None,
        }
    }
}
