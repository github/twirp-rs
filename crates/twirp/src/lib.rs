#[cfg(feature = "client")]
pub mod client;

mod body;

pub mod error;
pub mod headers;
pub mod server;

#[cfg(any(test, feature = "test-support"))]
pub mod test;

pub use body::*;
pub use client::{Client, ClientBuilder, ClientError, Middleware, Next, Result};
pub use error::*; // many constructors like `invalid_argument()`
pub use server::{serve, Router, Timings};

// Re-export `reqwest` so that it's easy to implement middleware.
pub use reqwest;

// Re-export `url so that the generated code works without additional dependencies beyond just the `twirp` crate.
pub use url;
