pub mod registry;
pub mod session;

pub use registry::{AgentDefinition, find_agent, is_command_available, load_agents};
pub use session::{AgentOptions, AgentResult, execute_agent};

