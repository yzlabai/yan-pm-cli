pub mod backend;
pub mod backends;
pub mod registry;
pub mod session;
pub mod state;

pub use backend::AgentBackend;
pub use registry::{find_agent, find_backend, is_command_available, load_agents};
pub use session::{execute_agent, AgentOptions, AgentResult};
