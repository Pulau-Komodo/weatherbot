use std::fmt::Display;

/// A `Box<dyn std::error::Error + Send>` to be logged, or a `String` to be sent to the application user.
///
/// It doesn't implement the Error trait because that conflicts with the blanket `From<E>` implementation.
#[derive(Debug)]
pub enum Error {
	/// An error message to be sent to the application user.
	Friendly(String),
	/// An error message to be logged but not sent to the application user.
	Unfriendly(Box<dyn std::error::Error + Send>),
}

impl Error {
	/// An error message to be sent to the application user.
	pub fn friendly<S>(text: S) -> Self
	where
		S: Into<String>,
	{
		Self::Friendly(text.into())
	}
	/// A custom message to be logged but not sent to the application user.
	pub fn custom_unfriendly<S>(text: S) -> Self
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

/// This type just exists so I can easily store `String`s in my custom `Error::unfriendly`.
#[derive(Debug)]
pub struct CustomError(String);

impl Display for CustomError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		self.0.fmt(f)
	}
}

impl std::error::Error for CustomError {}
