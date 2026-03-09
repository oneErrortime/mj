use super::ActionEndpoint;
use crate::error::Result;

pub trait LoadStrategy: Send + Sync {
    fn select<'a>(&self, endpoints: &'a [ActionEndpoint]) -> Result<&'a ActionEndpoint>;
}
