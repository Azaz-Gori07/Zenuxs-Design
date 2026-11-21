//! Chat preferences: how the model's reasoning is shown.
//!
//! The three modes match the way people read a long design answer: `collapsed` keeps it behind a
//! disclosure, `while-thinking` follows the reasoning as it streams, and `expanded` leaves it open
//! after the answer lands.

use serde::{Deserialize, Serialize};

/// How reasoning is shown in the chat.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReasoningDisplay {
    /// Behind a disclosure the person opens.
    #[default]
    Collapsed,
    /// Follow it while the model is thinking, then hide it.
    WhileThinking,
    /// Leave it open after the answer.
    Expanded,
}

impl ReasoningDisplay {
    /// The id for the settings and the wire.
    pub fn id(self) -> &'static str {
        match self {
            Self::Collapsed => "collapsed",
            Self::WhileThinking => "while-thinking",
            Self::Expanded => "expanded",
        }
    }

    /// The mode with this id; anything unknown is `collapsed`.
    pub fn from_id(id: &str) -> Self {
        match id {
            "while-thinking" => Self::WhileThinking,
            "expanded" => Self::Expanded,
            _ => Self::Collapsed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_unknown_falls_back() {
        for mode in [ReasoningDisplay::Collapsed, ReasoningDisplay::WhileThinking, ReasoningDisplay::Expanded] {
            assert_eq!(ReasoningDisplay::from_id(mode.id()), mode);
        }
        assert_eq!(ReasoningDisplay::from_id("nope"), ReasoningDisplay::Collapsed);
    }
}
