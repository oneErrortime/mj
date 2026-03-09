//! Load-balancing strategy traits.

use super::ActionEndpoint;
use crate::error::Result;

/// Trait for custom load-balancing strategies.
pub trait LoadStrategy: Send + Sync {
    fn select<'a>(&self, endpoints: &'a [ActionEndpoint]) -> Result<&'a ActionEndpoint>;
}
