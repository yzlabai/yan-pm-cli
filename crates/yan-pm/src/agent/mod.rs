pub mod registry;
pub mod session;

pub use registry::{find_agent, is_command_available, load_agents, AgentDefinition};
pub use session::{execute_agent, AgentOptions, AgentResult};
