#![allow(clippy::too_many_arguments, clippy::module_inception)]

pub mod config;
pub mod agent {
    pub mod backend;
    pub mod backends;
    pub mod registry;
    pub mod state;
}
pub mod daemon {
    pub mod event_store;
}
