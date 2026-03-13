use alloc::collections::VecDeque;
use alloc::sync::Arc;

use crate::endpoint::Endpoint;

#[derive(Clone)]
pub enum Capability {
    Endpoint(Arc<Endpoint>),
    // possibly others later
}
