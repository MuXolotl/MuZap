use std::io;

use thiserror::Error;

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("{0}")]
    Msg(String),

    #[error("Ошибка ввода/вывода: {0}")]
    Io(#[from] io::Error),

    #[error("Некорректный аргумент: {0}")]
    Arg(String),
}

impl CoreError {
    pub fn msg(s: impl Into<String>) -> Self {
        Self::Msg(s.into())
    }

    pub fn arg(s: impl Into<String>) -> Self {
        Self::Arg(s.into())
    }
}
