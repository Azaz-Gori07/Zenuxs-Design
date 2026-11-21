//! External agents an ACP connection can spawn.
//!
//! An ACP (Agent Client Protocol) agent is a command-line program that speaks JSON-RPC over its
//! standard input and output. The app starts it, initializes a session, sends the prompt, and
//! maps the session's updates into the chat's stream. Three agents are known here; a person can
//! use any command by putting it in a connection's base URL field.

use serde::{Deserialize, Serialize};

/// One agent FigCraft knows how to start.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ACPAgentDef {
    /// The id, as it appears in the connection's model id.
    pub id: String,
    /// The name shown in the picker.
    pub name: String,
    /// The command to run.
    pub command: String,
    /// The command's arguments.
    #[serde(default)]
    pub args: Vec<String>,
    /// How to install it, when it is missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_command: Option<String>,
}

/// The agents FigCraft knows, in picker order.
pub fn agents() -> &'static [ACPAgentDef] {
    &AGENTS
}

/// The agent with this id.
pub fn agent(id: &str) -> Option<&'static ACPAgentDef> {
    AGENTS.iter().find(|a| a.id.eq_ignore_ascii_case(id))
}

static AGENTS: std::sync::LazyLock<Vec<ACPAgentDef>> = std::sync::LazyLock::new(|| {
    vec![
        ACPAgentDef {
            id: "claude-code".into(),
            name: "Claude Code".into(),
            command: "claude-agent-acp".into(),
            args: vec![],
            install_command: Some("npm i -g @agentclientprotocol/claude-agent-acp".into()),
        },
        ACPAgentDef {
            id: "codex".into(),
            name: "Codex".into(),
            command: "codex-acp".into(),
            args: vec![],
            install_command: Some("npm i -g @zed-industries/codex-acp".into()),
        },
        ACPAgentDef {
            id: "gemini-cli".into(),
            name: "Gemini CLI".into(),
            command: "gemini".into(),
            args: vec!["--acp".into()],
            install_command: Some("npm i -g @google/gemini-cli".into()),
        },
    ]
});

/// The protocol version this client speaks.
pub const PROTOCOL_VERSION: u32 = 1;

/// The most lines of agent output kept for the diagnostic log.
pub const MAX_DEBUG_LINES: usize = 200;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agents_have_commands_and_how_to_install_them() {
        for a in agents() {
            assert!(!a.command.is_empty(), "{} names a command", a.id);
            assert!(a.install_command.as_ref().is_some_and(|s| s.contains("npm i")), "{} says how to install", a.id);
        }
    }

    #[test]
    fn ids_are_found_without_case() {
        assert_eq!(agent("CLAUDE-CODE").map(|a| a.id.as_str()), Some("claude-code"));
        assert!(agent("nope").is_none());
    }
}
