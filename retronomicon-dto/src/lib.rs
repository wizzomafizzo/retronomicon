pub mod encodings;
pub mod error;
pub mod params;
pub mod types;

pub mod artifact;
pub mod auth;
pub mod cores;
pub mod games;
pub mod images;
pub mod platforms;
pub mod systems;
pub mod tags;
pub mod teams;
pub mod user;

pub mod client;

pub use client::routes;
use serde::{Deserialize, Deserializer, Serializer};

pub mod reexports {
    pub use strum;
}

/// The expected response of an end point that does not return anything.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct Ok;

impl serde::Serialize for Ok {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bool(true)
    }
}

impl<'de> Deserialize<'de> for Ok {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(serde::de::IgnoredAny)?;
        Result::Ok(Self)
    }
}

/// A JWT authentication token.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct AuthTokenResponse {
    /// The token itself.
    pub token: String,
}

pub use error::JsonError;
