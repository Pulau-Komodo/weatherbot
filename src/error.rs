use std::fmt::Display;

#[derive(Debug)]
pub enum Error {
	Friendly(String),
	Unfriendly(Box<dyn std::error::Error + Send>),
}

impl Error {
	pub fn friendly<S>(text: S) -> Self
	where
		S: Into<String>,
	{
		Self::Friendly(text.into())
	}
	pub fn custom<S>(text: S) -> Self
	where
		S: Into<String>,
	{
		Self::Unfriendly(Box::new(CustomError(text.into())))
	}
}

impl Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Friendly(text) => text.fmt(f),
			Self::Unfriendly(error) => error.fmt(f),
		}
	}
}

//impl std::error::Error for Error {}

impl<E> From<E> for Error
where
	E: std::error::Error + Send + 'static,
{
	fn from(value: E) -> Self {
		Self::Unfriendly(Box::new(value))
	}
}

#[derive(Debug)]
pub struct CustomError(String);

impl Display for CustomError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		self.0.fmt(f)
	}
}

impl std::error::Error for CustomError {}
