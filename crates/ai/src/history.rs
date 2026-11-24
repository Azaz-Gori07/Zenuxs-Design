//! Saved chat conversations.
//!
//! A session holds one conversation in front; this is the shelf they are kept on between runs.
//! Each one remembers the document it was about (a `uid`, or the file's path), so the chat panel
//! can offer "this document" first without claiming a conversation belongs to a file it cannot
//! name. A conversation is bounded in both messages and count: a chat history is not an archive
//! of everything a model has ever written.

use serde::{Deserialize, Serialize};

use crate::transport::ChatMessage;

/// The most conversations kept.
pub const MAX_CONVERSATIONS: usize = 50;

/// The most messages a saved conversation keeps.
pub const MAX_SAVED_MESSAGES: usize = 200;

/// One saved conversation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatConversation {
    /// The id (`chat-…`), unique among the saved ones.
    pub id: String,
    /// The title shown in the list (the first user message, shortened).
    pub title: String,
    /// The document id it was about, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<String>,
    /// The messages, oldest first.
    pub messages: Vec<ChatMessage>,
    /// When it was last written, as the number of seconds since the epoch.
    #[serde(default)]
    pub updated_at: u64,
}

impl ChatConversation {
    /// A title from the first user message, shortened to a line.
    pub fn title_of(messages: &[ChatMessage]) -> String {
        let first = messages.iter().find(|m| m.role == crate::transport::Role::User).map(|m| m.content.trim()).unwrap_or_default();
        let line = first.lines().next().unwrap_or("New chat").trim();
        let mut title: String = line.chars().take(60).collect();
        if line.chars().count() > 60 {
            title.push('…');
        }
        if title.is_empty() { "New chat".into() } else { title }
    }

    /// Add a message and refresh the title from the first one.
    pub fn push(&mut self, message: ChatMessage) {
        self.messages.push(message);
        let title = Self::title_of(&self.messages);
        if !title.is_empty() {
            self.title = title;
        }
        while self.messages.len() > MAX_SAVED_MESSAGES {
            let users: Vec<usize> =
                self.messages.iter().enumerate().filter(|(_, m)| m.role == crate::transport::Role::User).map(|(i, _)| i).collect();
            let cut = match users.as_slice() {
                // Drop the first user turn and everything the model answered with it.
                [_, second, ..] => *second,
                // Only one turn left: drop any leading orphan tool messages before it.
                [first] if *first > 0 => *first,
                _ => break,
            };
            self.messages.drain(..cut);
        }
    }
}

/// The saved conversations and which one is in front.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatHistory {
    /// The conversations, newest first.
    #[serde(default)]
    pub conversations: Vec<ChatConversation>,
    /// The id of the one in front.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    /// Whether the panel shows only this document's chats (`true`) or all of them.
    #[serde(default = "yes")]
    pub this_document: bool,
}

fn yes() -> bool {
    true
}

impl Default for ChatHistory {
    fn default() -> Self {
        Self { conversations: Vec::new(), active: None, this_document: true }
    }
}

impl ChatHistory {
    /// The next id none of the saved ones uses.
    fn new_id(&self) -> String {
        for n in 1..(MAX_CONVERSATIONS + 2) {
            let id = format!("chat-{n}");
            if !self.conversations.iter().any(|c| c.id == id) {
                return id;
            }
        }
        format!("chat-{}", self.conversations.len() + 1)
    }

    /// Start a new conversation, putting the old one on the shelf when it has messages.
    pub fn new_conversation(&mut self, current: Option<&ChatConversation>, document: Option<&str>, now: u64) -> String {
        if let Some(current) = current
            && current.messages.iter().any(|m| m.role == crate::transport::Role::User)
        {
            self.save(current.clone(), now);
        }
        let id = self.new_id();
        self.active = Some(id.clone());
        // A chat about the document in front is listed under it.
        if self.this_document && document.is_none() {
            self.this_document = false;
        }
        id
    }

    /// Save a conversation, replacing one with the same id and keeping the list bounded.
    pub fn save(&mut self, conversation: ChatConversation, now: u64) {
        let mut conversation = conversation;
        conversation.updated_at = now;
        self.conversations.retain(|c| c.id != conversation.id);
        self.conversations.insert(0, conversation);
        while self.conversations.len() > MAX_CONVERSATIONS {
            // Keep the active one even when it is the oldest.
            let last = self.conversations.len() - 1;
            if self.conversations[last].id == self.active.as_deref().unwrap_or_default() && last > 0 {
                self.conversations.remove(last - 1);
            } else {
                self.conversations.remove(last);
            }
        }
    }

    /// A saved conversation by id.
    pub fn get(&self, id: &str) -> Option<&ChatConversation> {
        self.conversations.iter().find(|c| c.id == id)
    }

    /// Open a saved conversation (the caller loads its messages into the session).
    pub fn open(&mut self, id: &str) -> Option<ChatConversation> {
        let conversation = self.get(id)?.clone();
        self.active = Some(id.to_string());
        Some(conversation)
    }

    /// Rename one.
    pub fn rename(&mut self, id: &str, title: &str) -> bool {
        let Some(conversation) = self.conversations.iter_mut().find(|c| c.id == id) else { return false };
        let title = title.trim();
        if title.is_empty() {
            return false;
        }
        conversation.title = title.chars().take(80).collect();
        true
    }

    /// Forget one; the active one becomes none when it was the one deleted.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.conversations.len();
        self.conversations.retain(|c| c.id != id);
        if self.active.as_deref() == Some(id) {
            self.active = None;
        }
        self.conversations.len() != before
    }

    /// The list the panel shows, newest first, optionally only one document's.
    pub fn visible(&self, document: Option<&str>) -> Vec<&ChatConversation> {
        self.conversations.iter().filter(|c| !self.this_document || document.is_none() || c.document.as_deref() == document).collect()
    }

    /// Search by title or by any message's text (case-insensitive).
    pub fn search(&self, query: &str) -> Vec<&ChatConversation> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return self.conversations.iter().collect();
        }
        self.conversations
            .iter()
            .filter(|c| c.title.to_lowercase().contains(&needle) || c.messages.iter().any(|m| m.content.to_lowercase().contains(&needle)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{ChatMessage, Role};

    fn history() -> ChatHistory {
        ChatHistory { this_document: false, ..Default::default() }
    }

    #[test]
    fn a_title_comes_from_the_first_user_message() {
        let messages = vec![ChatMessage::user("Design a landing page\nwith three sections")];
        assert_eq!(ChatConversation::title_of(&messages), "Design a landing page");
    }

    #[test]
    fn a_long_title_is_shortened() {
        let text = "x".repeat(100);
        let title = ChatConversation::title_of(&[ChatMessage::user(text)]);
        assert_eq!(title.chars().count(), 61);
        assert!(title.ends_with('…'));
    }

    #[test]
    fn saving_a_conversation_keeps_it_by_id() {
        let mut h = history();
        let conversation =
            ChatConversation { id: "chat-1".into(), title: "First".into(), document: None, messages: vec![ChatMessage::user("hi")], updated_at: 0 };
        h.save(conversation, 100);
        assert_eq!(h.conversations.len(), 1);
        assert_eq!(h.conversations[0].updated_at, 100);
        h.save(ChatConversation { id: "chat-1".into(), title: "Renamed".into(), document: None, messages: vec![], updated_at: 0 }, 200);
        assert_eq!(h.conversations.len(), 1, "the same id replaced");
        assert_eq!(h.conversations[0].title, "Renamed");
    }

    #[test]
    fn a_new_chat_shelves_the_old_one() {
        let mut h = history();
        let current =
            ChatConversation { id: "chat-1".into(), title: "First".into(), document: None, messages: vec![ChatMessage::user("hi")], updated_at: 0 };
        let id = h.new_conversation(Some(&current), None, 50);
        assert_ne!(id, current.id);
        assert_eq!(h.conversations.len(), 1);
        assert_eq!(h.conversations[0].title, "First");
        assert_eq!(h.active.as_deref(), Some(id.as_str()));
    }

    #[test]
    fn an_empty_chat_is_not_shelved() {
        let mut h = history();
        let current = ChatConversation { id: "chat-1".into(), title: "New chat".into(), document: None, messages: vec![], updated_at: 0 };
        h.new_conversation(Some(&current), None, 50);
        assert!(h.conversations.is_empty());
    }

    #[test]
    fn opening_renaming_and_removing() {
        let mut h = history();
        h.save(
            ChatConversation {
                id: "chat-1".into(),
                title: "First".into(),
                document: Some("7".into()),
                messages: vec![ChatMessage::user("hello world")],
                updated_at: 0,
            },
            0,
        );
        assert!(h.open("chat-1").is_some());
        assert_eq!(h.active.as_deref(), Some("chat-1"));
        assert!(h.rename("chat-1", "  Better title  "));
        assert_eq!(h.get("chat-1").map(|c| c.title.as_str()), Some("Better title"));
        assert!(!h.rename("chat-1", ""));
        assert!(h.remove("chat-1"));
        assert!(h.active.is_none());
    }

    #[test]
    fn search_looks_in_titles_and_messages() {
        let mut h = history();
        h.save(
            ChatConversation {
                id: "chat-1".into(),
                title: "Landing".into(),
                document: None,
                messages: vec![ChatMessage::user("make a pricing table")],
                updated_at: 0,
            },
            0,
        );
        assert_eq!(h.search("landing").len(), 1);
        assert_eq!(h.search("PRICING").len(), 1);
        assert!(h.search("nothing here").is_empty());
    }

    #[test]
    fn this_document_filters_by_document() {
        let mut h = ChatHistory::default();
        h.save(
            ChatConversation { id: "a".into(), title: "A".into(), document: Some("7".into()), messages: vec![ChatMessage::user("a")], updated_at: 0 },
            0,
        );
        h.save(
            ChatConversation { id: "b".into(), title: "B".into(), document: Some("8".into()), messages: vec![ChatMessage::user("b")], updated_at: 0 },
            0,
        );
        assert_eq!(h.visible(Some("7")).len(), 1);
        h.this_document = false;
        assert_eq!(h.visible(None).len(), 2);
    }

    #[test]
    fn a_saved_chat_never_ends_on_an_orphan_tool_answer() {
        let mut c = ChatConversation { id: "c".into(), title: "C".into(), document: None, messages: vec![], updated_at: 0 };
        c.push(ChatMessage::tool_result("x", "{}"));
        for _ in 0..(MAX_SAVED_MESSAGES + 10) {
            c.push(ChatMessage::user("more"));
        }
        assert!(c.messages.len() <= MAX_SAVED_MESSAGES);
        assert_ne!(c.messages.first().map(|m| m.role), Some(Role::Tool));
    }
}
