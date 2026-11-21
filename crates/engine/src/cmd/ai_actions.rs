//! Named AI actions: one prompt, the same chat and tools underneath.
//!
//! A person does not have to phrase a request to rename the selected layers or to review a design:
//! the menu item does. The action is a command (so the CLI, the control channel and MCP reach it
//! too); it queues a prepared message through `ai.chat.send`, and the interface sends it. The
//! action never mutates the document itself — the model does, through ordinary commands.

use serde_json::{Value, json};

use super::{CommandSpec, ok};
use crate::{Result, Session, spec};

/// The prompts, kept with the command ids so a test can keep them all real.
const ACTIONS: &[(&str, &str, &str)] = &[
    (
        "ai.action.firstDraft",
        "Make a first draft",
        "Design a first draft on the current page. Follow the craft rules in your instructions: work top-down, create the containers first, fill them one at a time, and check each section with get_node before moving on. Finish with a two-line summary of the frames and the accent you used.",
    ),
    (
        "ai.action.renameLayers",
        "Rename layers with AI",
        "Rename the selected layers to describe what each one is, in two to four words. Use the layer's content, its role in the screen and the document's naming style. Change only names — no geometry, fills or structure. Use batch so the whole rename is one undo step.",
    ),
    (
        "ai.action.reviewDesign",
        "Review the design",
        "Review the open page against the craft rules in your instructions: hierarchy, spacing grid, contrast, containment and consistency. Do not change anything. List the findings by severity, naming the layer ids and the exact fix for each, then say what already works.",
    ),
    (
        "ai.action.accessibility",
        "Check accessibility",
        "Check the open page for accessibility: text contrast, focus indication, touch target sizes, labels and states that rely on colour alone. Do not change anything. List each issue by layer id with the minimum fix, and say what already passes.",
    ),
];

/// Run an action by its id: queue the prepared prompt in the chat.
fn run_action(s: &mut Session, id: &str) -> Result<Value> {
    let prompt =
        ACTIONS.iter().find(|(action, _, _)| *action == id).map(|(_, _, p)| *p).ok_or_else(|| crate::EngineError::UnknownCommand(id.to_string()))?;
    s.execute("ai.chat.send", &json!({"text": prompt}))?;
    ok()
}

pub fn specs() -> Vec<CommandSpec> {
    ACTIONS
        .iter()
        .map(|(id, label, _)| {
            let run: crate::cmd::RunFn = match *id {
                "ai.action.firstDraft" => first_draft,
                "ai.action.renameLayers" => rename_layers,
                "ai.action.reviewDesign" => review_design,
                _ => accessibility,
            };
            spec!(id, label, "{}  queues the prepared prompt in the AI chat; the model's changes are ordinary undoable commands", false, run)
        })
        .collect()
}

fn first_draft(s: &mut Session, _: &Value) -> Result<Value> {
    run_action(s, "ai.action.firstDraft")
}
fn rename_layers(s: &mut Session, _: &Value) -> Result<Value> {
    run_action(s, "ai.action.renameLayers")
}
fn review_design(s: &mut Session, _: &Value) -> Result<Value> {
    run_action(s, "ai.action.reviewDesign")
}
fn accessibility(s: &mut Session, _: &Value) -> Result<Value> {
    run_action(s, "ai.action.accessibility")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_queues_a_prompt() {
        for (id, label, prompt) in ACTIONS {
            assert!(!id.is_empty() && !label.is_empty() && prompt.len() > 40, "{} is a full action", id);
        }
    }

    #[test]
    fn an_action_can_be_run_and_queues_a_chat_message() {
        let mut s = Session::empty();
        let _ = s.execute("file.new", &json!({}));
        let out = s.execute("ai.action.renameLayers", &json!({})).expect("the action runs");
        assert_eq!(out["ok"], true);
        assert!(s.chat.pending_send, "the interface has a message to send");
        let messages = s.chat_messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].content.contains("Rename the selected layers"));
    }
}
