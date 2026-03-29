use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Idle,
    Connecting,
    Ready,
    Error,
    Stopped,
}

impl fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionState::Idle => write!(f, "idle"),
            ConnectionState::Connecting => write!(f, "connecting"),
            ConnectionState::Ready => write!(f, "ready"),
            ConnectionState::Error => write!(f, "error"),
            ConnectionState::Stopped => write!(f, "stopped"),
        }
    }
}

#[allow(dead_code)]
impl ConnectionState {
    pub fn transition(
        &self,
        to: ConnectionState,
        error_code: Option<AgentErrorCode>,
    ) -> Option<StateTransition> {
        let valid = matches!(
            (self, to),
            (ConnectionState::Idle, ConnectionState::Connecting)
                | (ConnectionState::Connecting, ConnectionState::Ready)
                | (ConnectionState::Connecting, ConnectionState::Error)
                | (ConnectionState::Ready, ConnectionState::Error)
                | (ConnectionState::Ready, ConnectionState::Stopped)
                | (ConnectionState::Error, ConnectionState::Connecting)
                | (ConnectionState::Error, ConnectionState::Stopped)
        );

        if valid {
            Some(StateTransition {
                from: *self,
                to,
                error_code,
            })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentErrorCode {
    AgentNotFound,
    AgentSpawnFailed,
    AgentTimeout,
    AgentCrashed,
    ProtocolError,
}

impl AgentErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentErrorCode::AgentNotFound => "AGENT_NOT_FOUND",
            AgentErrorCode::AgentSpawnFailed => "AGENT_SPAWN_FAILED",
            AgentErrorCode::AgentTimeout => "AGENT_TIMEOUT",
            AgentErrorCode::AgentCrashed => "AGENT_CRASHED",
            AgentErrorCode::ProtocolError => "PROTOCOL_ERROR",
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct StateTransition {
    pub from: ConnectionState,
    pub to: ConnectionState,
    pub error_code: Option<AgentErrorCode>,
}
