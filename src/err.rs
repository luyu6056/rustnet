use std::time::SystemTimeError;
use thiserror::Error;
use tokio::time::error::Elapsed;

#[derive(Error, Debug, Clone)]
pub enum NetError {
    #[error("IoError: {0}")]
    IoError(String),

    #[error("{0}")]
    Custom(String),

    #[error("tcp read timeout")]
    TcpReadTimeout,

    #[error("tcp write timeout")]
    TcpWriteTimeout,

    #[error("tcp disconnected")]
    TcpDisconnected,

    #[error("UknowError")]
    UknowError,

    #[error("{0} Address error")]
    AddressError(String),

    #[error("package is too large")]
    LargePackage,

    #[error("ShutdownServer,reason: {0}")]
    ShutdownServer(String),

    #[error("no error")]
    None,

    #[error("close no error")]
    Close,

    #[error("channel {0}")]
    Channel(String),

    #[error("{0}")]
    FormatErr(String),

    ///Server未启动
    #[error("No server")]
    NoStartServer,
}
impl NetError {
    pub fn new_with_string(s: String) -> NetError {
        NetError::Custom(s)
    }
}
impl std::convert::From<std::io::Error> for NetError {
    fn from(err: std::io::Error) -> Self {
        NetError::IoError(err.to_string())
    }
}
impl std::convert::From<Elapsed> for NetError {
    fn from(err: Elapsed) -> Self {
        NetError::Custom(err.to_string())
    }
}
impl<T> std::convert::From<tokio::sync::mpsc::error::SendError<T>> for NetError {
    fn from(err: tokio::sync::mpsc::error::SendError<T>) -> Self {
        NetError::Channel(err.to_string())
    }
}
impl<T> std::convert::From<tokio::sync::mpsc::error::TrySendError<T>> for NetError {
    fn from(err: tokio::sync::mpsc::error::TrySendError<T>) -> Self {
        NetError::Channel(err.to_string())
    }
}

impl std::convert::From<SystemTimeError> for NetError {
    fn from(err: SystemTimeError) -> Self {
        NetError::Custom(err.to_string())
    }
}


use std::string::FromUtf8Error;
impl std::convert::From<FromUtf8Error> for NetError {
    fn from(err: FromUtf8Error) -> Self {
        NetError::FormatErr(format!("byte转utf8错误,{}", err.to_string()))
    }
}

