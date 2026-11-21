//! The design skills: what the agent should know before it touches the canvas.
//!
//! A skill is *when and why*; a command is *how*. The registry holds the skills, the router
//! decides which ones a message activates, and the activated ones are appended to the model
//! system instructions as a compact block.
//!
//! Two things are kept apart on purpose:
//!
//! - a **semantic skill** is structured knowledge with a slash command, a set of triggers and a
//!   dependency chain. It is what the router activates.
//! - a **prompt skill** is a plain block of text a person writes and switches on. It is appended
//!   to the system instructions as it is, with no routing.
//!
//! The router is the same in both cases: a leading `/command` names a skill outright, and plain
//! language is matched against the skills' triggers. What the router produces is then ordered so
//! that a skill's dependencies come before it, and the block is written for the model.

pub mod definitions;
pub mod prompt;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub use definitions::builtin_skills;
pub use prompt::{PromptSkill, PromptSkillStore, active_prompt_text};

/// The kinds of skill there are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillCategory {
    /// Whole screens and pages.
    Design,
    /// A recurring screen shape (a login, a feed, a pricing table).
    Patterns,
    /// Wiring screens into flows.
    Prototype,
    /// How things move.
    Motion,
    /// The states a thing can be in.
    State,
    /// Conventions that span a whole system.
    Systems,
    /// Adapting a layout to a different size.
    Responsive,
}

impl SkillCategory {
    /// Every category, in the order the settings list them.
    pub fn all() -> &'static [Self] {
        &[Self::Design, Self::Patterns, Self::Prototype, Self::Motion, Self::State, Self::Systems, Self::Responsive]
    }

    /// The category's id, as it appears in a slash command.
    pub fn id(self) -> &'static str {
        match self {
            Self::Design => "design",
            Self::Patterns => "patterns",
            Self::Prototype => "prototype",
            Self::Motion => "motion",
            Self::State => "state",
            Self::Systems => "systems",
            Self::Responsive => "responsive",
        }
    }

    /// The category with this id.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::all().iter().copied().find(|c| c.id() == id)
    }
}

/// One representative request and what it means.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillExample {
    /// What a person might write.
    pub prompt: String,
    /// What the request is asking for.
    pub intent: String,
}

/// A skill the router can activate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Skill {
    /// The skill's stable id (`prototype.bottom-sheet`).
    pub id: String,
    /// The slash command that names it, without the slash.
    pub name: String,
    /// What kind of skill it is.
    pub category: SkillCategory,
    /// One line saying what it is for.
    pub description: String,
    /// The version, so a skill that changes can say so.
    pub version: String,
    /// The phrases that strongly select it.
    #[serde(default)]
    pub triggers: Vec<String>,
    /// Other names it answers to.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// The skills this one pulls in, ordered before it.
    #[serde(default)]
    pub requires: Vec<String>,
    /// The defaults applied only where the person gave no value.
    #[serde(default)]
    pub defaults: BTreeMap<String, String>,
    /// The commands this skill routes to. The rest stay available.
    #[serde(default)]
    pub operations: Vec<String>,
    /// The design knowledge itself: the vocabulary, when and when not, the failure modes.
    pub knowledge: String,
    /// The checks the agent must run on its own work.
    #[serde(default)]
    pub validation: Vec<String>,
    /// Example requests and what they mean.
    #[serde(default)]
    pub examples: Vec<SkillExample>,
}

/// A slash command a person can type.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillCommand {
    /// The command, without the slash.
    pub command: String,
    /// One line saying what it does.
    pub description: String,
    /// The skill it belongs to; `None` for a category command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
}

/// A problem found while routing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillIssue {
    /// The skill the problem is about, or the command that was not known.
    pub skill_id: String,
    /// What to say about it.
    pub message: String,
}

/// What the router may know about the selection.
///
/// Node *types* only, never node content: routing stays away from the document.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RoutingContext {
    /// The types of the selected nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_node_types: Vec<String>,
    /// Whether anything is selected.
    #[serde(default)]
    pub has_selection: bool,
}

/// How one message should be handled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AmbiguityAction {
    /// Do what the message says.
    Execute,
    /// Work out what it means and do it.
    Infer,
    /// Ask which element it means.
    Clarify,
}

impl AmbiguityAction {
    /// The action's id.
    pub fn id(self) -> &'static str {
        match self {
            Self::Execute => "execute",
            Self::Infer => "infer",
            Self::Clarify => "clarify",
        }
    }
}

/// The routing decision for one message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillAmbiguity {
    /// What to do.
    pub action: AmbiguityAction,
    /// Why.
    pub reason: String,
}

/// What the router worked out for one message.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SkillRoute {
    /// The skills an explicit `/command` named.
    #[serde(default)]
    pub explicit: Vec<String>,
    /// The skills plain language matched.
    #[serde(default)]
    pub matched: Vec<String>,
    /// Every activated skill, dependencies first.
    #[serde(default)]
    pub ordered: Vec<String>,
    /// A `/command` that names no skill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unknown_command: Option<String>,
    /// The commands close to the unknown one.
    #[serde(default)]
    pub suggestions: Vec<String>,
    /// The problems found while routing.
    #[serde(default)]
    pub issues: Vec<SkillIssue>,
    /// How the message should be handled.
    #[serde(default)]
    pub ambiguity: SkillAmbiguity,
    /// What the message is about, when it points at the selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl Default for SkillAmbiguity {
    fn default() -> Self {
        Self { action: AmbiguityAction::Infer, reason: "natural-language intent".into() }
    }
}

/// The marker the skill block starts with.
pub const SKILL_CONTEXT_MARKER: &str = "\n\n[Design skills activated for this request]\n";

/// The registry of skills and the router over it.
pub struct SkillRouter {
    skills: Vec<Skill>,
    by_id: BTreeMap<String, usize>,
    /// Command name to skill index.
    by_command: BTreeMap<String, usize>,
    commands: Vec<SkillCommand>,
}

impl Default for SkillRouter {
    fn default() -> Self {
        Self::new(builtin_skills())
    }
}

impl SkillRouter {
    /// A router over these skills.
    pub fn new(skills: &[Skill]) -> Self {
        let mut by_id = BTreeMap::new();
        let mut by_command = BTreeMap::new();
        for (index, skill) in skills.iter().enumerate() {
            by_id.insert(skill.id.clone(), index);
            by_command.insert(skill.name.clone(), index);
            for alias in &skill.aliases {
                by_command.entry(alias.clone()).or_insert(index);
            }
        }
        let mut commands: Vec<SkillCommand> = by_command
            .iter()
            .filter(|(command, _)| is_slash_token(command))
            .map(|(command, index)| SkillCommand {
                command: command.clone(),
                description: skills[*index].description.clone(),
                skill_id: Some(skills[*index].id.clone()),
            })
            .collect();
        for category in SkillCategory::all() {
            if !by_command.contains_key(category.id()) {
                commands.push(SkillCommand {
                    command: category.id().to_string(),
                    description: format!("Activate every {} skill in one route.", category.id()),
                    skill_id: None,
                });
            }
        }
        commands.sort_by(|a, b| a.command.cmp(&b.command));
        Self { skills: skills.to_vec(), by_id, by_command, commands }
    }

    /// The skills, in registry order.
    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }

    /// The skill with this id.
    pub fn skill(&self, id: &str) -> Option<&Skill> {
        self.by_id.get(id).and_then(|i| self.skills.get(*i))
    }

    /// Every slash command a person can type.
    pub fn commands(&self) -> &[SkillCommand] {
        &self.commands
    }

    /// The skills of one category.
    pub fn skills_in_category(&self, category: SkillCategory) -> Vec<&Skill> {
        self.skills.iter().filter(|s| s.category == category).collect()
    }

    /// The skill a command names, or every skill of a category.
    ///
    /// `None` when the command names nothing.
    pub fn resolve_command(&self, command: &str) -> Option<Vec<&Skill>> {
        let normalized = normalize_command(command);
        if let Some(index) = self.by_command.get(&normalized) {
            return self.skills.get(*index).map(|s| vec![s]);
        }
        if let Some(category) = SkillCategory::from_id(&normalized) {
            return Some(self.skills_in_category(category));
        }
        None
    }

    /// The problems with the registry itself: duplicate ids, unknown dependencies, cycles.
    ///
    /// An empty list means the registry is sound.
    pub fn registry_issues(&self) -> Vec<String> {
        let mut issues = Vec::new();
        let mut ids = BTreeSet::new();
        let mut commands = BTreeSet::new();

        for skill in &self.skills {
            if !ids.insert(skill.id.clone()) {
                issues.push(format!("duplicate id \"{}\"", skill.id));
            }
            if !is_semver(&skill.version) {
                issues.push(format!("{}: version \"{}\" is not semver", skill.id, skill.version));
            }
            for command in std::iter::once(&skill.name).chain(skill.aliases.iter()) {
                if normalize_command(command) != *command {
                    issues.push(format!("{}: command \"{command}\" is not normalized kebab-case", skill.id));
                }
                if !commands.insert(command.clone()) {
                    issues.push(format!("{}: duplicate command \"{command}\"", skill.id));
                }
            }
            if skill.description.trim().is_empty() {
                issues.push(format!("{}: missing description", skill.id));
            }
            if skill.knowledge.trim().is_empty() {
                issues.push(format!("{}: missing knowledge", skill.id));
            }
            if skill.operations.is_empty() {
                issues.push(format!("{}: missing operations", skill.id));
            }
            for dependency in &skill.requires {
                if !self.by_id.contains_key(dependency) {
                    issues.push(format!("{}: unknown requires \"{dependency}\"", skill.id));
                }
            }
            for example in &skill.examples {
                if example.prompt.trim().is_empty() || example.intent.trim().is_empty() {
                    issues.push(format!("{}: incomplete example", skill.id));
                }
            }
        }

        // A cycle would break the router's ordering.
        let mut visiting: BTreeSet<String> = BTreeSet::new();
        let mut done: BTreeSet<String> = BTreeSet::new();
        for skill in &self.skills {
            self.visit(skill.id.as_str(), &mut visiting, &mut done, &mut Vec::new(), &mut issues);
        }

        issues
    }

    /// Walk the dependency graph, reporting a cycle when one is found.
    fn visit(&self, id: &str, visiting: &mut BTreeSet<String>, done: &mut BTreeSet<String>, trail: &mut Vec<String>, issues: &mut Vec<String>) {
        if done.contains(id) {
            return;
        }
        if !visiting.insert(id.to_string()) {
            let mut path = trail.clone();
            path.push(id.to_string());
            issues.push(format!("dependency cycle: {}", path.join(" → ")));
            return;
        }
        if let Some(skill) = self.skill(id) {
            for dependency in &skill.requires {
                self.visit(dependency, visiting, done, trail, issues);
            }
        }
        visiting.remove(id);
        done.insert(id.to_string());
        trail.push(id.to_string());
    }

    /// Work out which skills a message activates.
    pub fn route(&self, text: &str, context: Option<&RoutingContext>) -> SkillRoute {
        let leading = parse_leading_command(text);
        let mut explicit = Vec::new();
        let mut matched = Vec::new();
        let mut unknown_command = None;

        let natural_text = leading.as_ref().map(|l| l.rest.as_str()).unwrap_or(text);
        for skill in self.natural_matches(natural_text) {
            if !matched.contains(&skill.id) {
                matched.push(skill.id.clone());
            }
        }

        if let Some(leading) = &leading
            && let Some(resolved) = self.resolve_command(&leading.command)
        {
            for skill in resolved {
                if !explicit.contains(&skill.id) {
                    explicit.push(skill.id.clone());
                }
            }
        } else if let Some(leading) = &leading {
            unknown_command = Some(leading.command.clone());
        }

        let suggestions = unknown_command.as_ref().map(|c| self.suggest_commands(c)).unwrap_or_default();
        let issues = unknown_command
            .as_ref()
            .map(|c| vec![SkillIssue { skill_id: c.clone(), message: unknown_command_message(c, &suggestions) }])
            .unwrap_or_default();

        let mut seeds: Vec<&Skill> = Vec::new();
        for id in explicit.iter().chain(matched.iter()) {
            if let Some(skill) = self.skill(id) {
                seeds.push(skill);
            }
        }

        SkillRoute {
            explicit,
            matched,
            ordered: self.compose(&seeds, context, text),
            unknown_command,
            suggestions,
            issues,
            ambiguity: self.resolve_ambiguity(text, context),
            hint: target_hint(text, context),
        }
    }

    /// The skills plain language matched.
    fn natural_matches(&self, text: &str) -> Vec<&Skill> {
        let tokens = tokens(text);
        if tokens.is_empty() {
            return Vec::new();
        }
        self.skills.iter().filter(|skill| skill.triggers.iter().chain(skill.aliases.iter()).any(|phrase| phrase_matches(phrase, &tokens))).collect()
    }

    /// Decide how the message should be handled.
    pub fn resolve_ambiguity(&self, text: &str, context: Option<&RoutingContext>) -> SkillAmbiguity {
        if parse_leading_command(text).is_some() {
            return SkillAmbiguity { action: AmbiguityAction::Execute, reason: "explicit skill command".into() };
        }
        if context.is_some_and(|c| c.has_selection) {
            return SkillAmbiguity { action: AmbiguityAction::Infer, reason: "reference resolves against the current selection".into() };
        }
        if mentions_any(text, ANAPHORA) {
            // "Build a pricing page and make it bold" introduces its own target, so asking which
            // element "it" means would stall a request that is already complete.
            if introduces_new_artifact(text) {
                return SkillAmbiguity { action: AmbiguityAction::Infer, reason: "reference resolves to the artifact this message creates".into() };
            }
            return SkillAmbiguity { action: AmbiguityAction::Clarify, reason: "reference without a selection to apply it to".into() };
        }
        SkillAmbiguity { action: AmbiguityAction::Infer, reason: "natural-language intent".into() }
    }

    /// The commands close to an unknown one, nearest first.
    pub fn suggest_commands(&self, unknown: &str) -> Vec<String> {
        let typed = unknown.to_lowercase();
        let mut scored: Vec<(String, usize)> = Vec::new();
        for command in &self.commands {
            let mut distance = edit_distance(&typed, &command.command, MAX_COMMAND_EDITS + 1);
            if command.command.starts_with(&typed) || typed.starts_with(&command.command) {
                distance = distance.min(1);
            }
            if distance <= MAX_COMMAND_EDITS {
                scored.push((command.command.clone(), distance));
            }
        }
        scored.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        scored.into_iter().take(3).map(|(command, _)| command).collect()
    }

    /// Pull in the dependencies, then order them so a dependency comes before its dependent.
    fn compose(&self, seeds: &[&Skill], context: Option<&RoutingContext>, text: &str) -> Vec<String> {
        let mut included: Vec<String> = Vec::new();
        let mut queue: Vec<String> = seeds.iter().map(|s| s.id.clone()).collect();
        while let Some(id) = queue.pop() {
            if included.contains(&id) {
                continue;
            }
            included.push(id.clone());
            if let Some(skill) = self.skill(&id) {
                for dependency in &skill.requires {
                    if !included.contains(dependency) {
                        queue.push(dependency.clone());
                    }
                }
            }
        }

        let explicit: BTreeSet<&str> = self.skills.iter().filter(|s| seeds.iter().any(|seed| seed.id == s.id)).map(|s| s.id.as_str()).collect();
        let preferred = self.preferred_skills(text, context);

        let registry_order: BTreeMap<&str, usize> = self.skills.iter().enumerate().map(|(i, s)| (s.id.as_str(), i)).collect();
        let mut pending: Vec<String> = included;
        let mut ordered: Vec<String> = Vec::new();

        while !pending.is_empty() {
            let ready: Vec<String> = pending
                .iter()
                .filter(|id| self.skill(id).map(|s| s.requires.iter().all(|d| !pending.contains(d))).unwrap_or(false))
                .cloned()
                .collect();
            if ready.is_empty() {
                // Unreachable with an acyclic registry; keep the rest in a fixed order.
                pending.sort();
                ordered.extend(pending);
                break;
            }
            let mut ready = ready;
            ready.sort_by(|a, b| {
                let explicit = explicit.contains(b.as_str()) as i32 - explicit.contains(a.as_str()) as i32;
                if explicit != 0 {
                    return explicit.cmp(&0);
                }
                let preferred = preferred.contains(b.as_str()) as i32 - preferred.contains(a.as_str()) as i32;
                if preferred != 0 {
                    return preferred.cmp(&0);
                }
                registry_order.get(a.as_str()).copied().unwrap_or(0).cmp(&registry_order.get(b.as_str()).copied().unwrap_or(0))
            });
            if let Some(next) = ready.first() {
                pending.retain(|id| id != next);
                ordered.push(next.clone());
            }
        }
        ordered
    }

    /// The skills that fit the selection better than the rest.
    ///
    /// An instance that talks about variants is state or prototype work, so those skills come
    /// first.
    fn preferred_skills(&self, text: &str, context: Option<&RoutingContext>) -> BTreeSet<String> {
        let mut preferred = BTreeSet::new();
        let Some(context) = context else { return preferred };
        if !context.has_selection {
            return preferred;
        }
        if !context.selected_node_types.iter().any(|t| t == "INSTANCE") {
            return preferred;
        }
        if !mentions_any(text, INSTANCE_TASK_TERMS) {
            return preferred;
        }
        for skill in &self.skills {
            if matches!(skill.category, SkillCategory::State | SkillCategory::Prototype) {
                preferred.insert(skill.id.clone());
            }
        }
        preferred
    }

    /// The block the model reads: what was activated and what to do about it.
    ///
    /// Empty when nothing matched and nothing failed to route: a simple request stays direct.
    /// An unknown `/command` always reaches the model, so it can say what it meant.
    pub fn build_context(&self, route: &SkillRoute) -> String {
        let blocks: Vec<String> = route.ordered.iter().filter_map(|id| self.skill(id)).map(|s| self.skill_block(s)).collect();
        if blocks.is_empty() && route.issues.is_empty() {
            return String::new();
        }

        let mut lines: Vec<String> = vec![SKILL_CONTEXT_MARKER.to_string()];
        lines.push(format!("Routing mode: {} — {}.", route.ambiguity.action.id(), route.ambiguity.reason));
        if let Some(hint) = &route.hint {
            lines.push(format!("Target: {hint}."));
        }
        if !route.issues.is_empty() {
            lines.push(String::new());
            lines.extend(route.issues.iter().map(|i| i.message.clone()));
        }
        if !blocks.is_empty() {
            lines.push(String::new());
            lines.push(
                "Follow the activated skill knowledge below for this request. Explicit user instructions override skill defaults. Validate the result against each activated skill's validation rules before replying."
                    .into(),
            );
            lines.push(String::new());
            lines.extend(blocks);
        }
        lines.join("\n")
    }

    /// One skill's block: the knowledge, the checks, the defaults, the commands and the examples.
    fn skill_block(&self, skill: &Skill) -> String {
        let mut lines = vec![format!("### {} (v{})", skill.id, skill.version), skill.knowledge.clone()];
        if !skill.validation.is_empty() {
            lines.push(String::new());
            lines.push("Validation:".into());
            lines.extend(skill.validation.iter().map(|rule| format!("- {rule}")));
        }
        if !skill.defaults.is_empty() {
            lines.push(String::new());
            lines.push("Defaults (apply only where the user gave no value):".into());
            lines.extend(skill.defaults.iter().map(|(key, value)| format!("- {key}: {value}")));
        }
        if !skill.operations.is_empty() {
            lines.push(String::new());
            lines.push("Operations:".into());
            lines.extend(skill.operations.iter().map(|operation| format!("- {operation}")));
        }
        if !skill.examples.is_empty() {
            lines.push(String::new());
            lines.push("Examples:".into());
            for example in skill.examples.iter().take(2) {
                lines.push(format!("- \"{}\" → {}", example.prompt, example.intent));
            }
        }
        if !skill.requires.is_empty() {
            lines.push(String::new());
            lines.push(format!("Requires: {}", skill.requires.join(", ")));
        }
        lines.join("\n")
    }
}

/// Remove the skill block from a text, keeping what came before it.
pub fn strip_skill_context(text: &str) -> String {
    match text.rfind(SKILL_CONTEXT_MARKER) {
        Some(index) => text[..index].to_string(),
        None => text.to_string(),
    }
}

/// Words that point at something else, and so need a target to resolve against.
const ANAPHORA: &[&str] = &["it", "its", "this", "that", "these", "those", "they", "them", "their", "selected", "current"];

/// Words that point at something that already exists.
const DEFINITE_REFERENCES: &[&str] = &["the", "this", "that", "these", "those", "current", "selected"];

/// Verbs that introduce a new artifact.
const CREATION_VERBS: &[&str] =
    &["build", "create", "design", "make", "generate", "draft", "compose", "redesign", "restyle", "sketch", "prototype", "draw", "layout"];

/// Nouns that name the artifact a creation request produces.
const ARTIFACT_NOUNS: &[&str] = &[
    "page",
    "screen",
    "app",
    "dashboard",
    "landing",
    "website",
    "site",
    "interface",
    "ui",
    "component",
    "hero",
    "section",
    "poster",
    "deck",
    "pricing",
    "portfolio",
    "onboarding",
    "flow",
    "form",
    "modal",
    "menu",
    "gallery",
    "feed",
    "profile",
    "checkout",
    "settings",
    "table",
    "card",
    "button",
    "header",
    "footer",
    "navbar",
    "sidebar",
    "state",
];

/// The words that mean an instance is being swapped or set to a state.
const INSTANCE_TASK_TERMS: &[&str] = &["variant", "variants", "state", "states", "swap", "swapping"];

/// How many edits an unknown command may be from a known one and still be suggested.
const MAX_COMMAND_EDITS: usize = 2;

/// The words of a text, lowercased.
fn tokens(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.to_lowercase()).collect()
}

/// The words of a phrase, lowercased, in order.
fn phrase_tokens(phrase: &str) -> Vec<String> {
    phrase.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.to_lowercase()).collect()
}

/// A leading `/command` and the text after it.
pub fn parse_leading_command(text: &str) -> Option<LeadingCommand> {
    let trimmed = text.trim_start();
    let rest = trimmed.strip_prefix('/')?;
    let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect();
    if name.is_empty() {
        return None;
    }
    let after = &rest[name.len()..];
    Some(LeadingCommand { command: normalize_command(&name), rest: after.trim_start().to_string() })
}

/// A message's leading `/command`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeadingCommand {
    /// The command, normalized.
    pub command: String,
    /// The text after it.
    pub rest: String,
}

/// Normalize a slash command: lower case, underscores as dashes.
pub fn normalize_command(command: &str) -> String {
    command.trim().to_lowercase().replace('_', "-")
}

/// Whether a phrase's every word is in the text.
fn phrase_matches(phrase: &str, text_tokens: &BTreeSet<String>) -> bool {
    let parts = phrase_tokens(phrase);
    !parts.is_empty() && parts.iter().all(|part| text_tokens.contains(part))
}

/// Whether the text says any of the words.
fn mentions_any(text: &str, vocabulary: &[&str]) -> bool {
    tokens(text).iter().any(|t| vocabulary.contains(&t.as_str()))
}

/// Whether a creation verb reaches an artifact noun without a definite reference in between.
fn introduces_new_artifact(text: &str) -> bool {
    let words = phrase_tokens(text);
    for (index, word) in words.iter().enumerate() {
        if !CREATION_VERBS.contains(&word.as_str()) {
            continue;
        }
        for step in (index + 1)..words.len().min(index + 5) {
            let word = words[step].as_str();
            if DEFINITE_REFERENCES.contains(&word) {
                break;
            }
            if ARTIFACT_NOUNS.contains(&word) {
                return true;
            }
        }
    }
    false
}

/// The distance between two words, giving up past `max`.
fn edit_distance(source: &str, target: &str, max: usize) -> usize {
    let source: Vec<char> = source.chars().collect();
    let target: Vec<char> = target.chars().collect();
    if source.len().abs_diff(target.len()) > max {
        return max + 1;
    }
    let mut previous: Vec<usize> = (0..=target.len()).collect();
    for row in 1..=source.len() {
        let mut current = vec![row];
        let mut row_minimum = row;
        for column in 1..=target.len() {
            let substitution = usize::from(source[row - 1] != target[column - 1]);
            let value = (previous[column] + 1).min(current[column - 1] + 1).min(previous[column - 1] + substitution);
            current.push(value);
            row_minimum = row_minimum.min(value);
        }
        if row_minimum > max {
            return max + 1;
        }
        previous = current;
    }
    previous.get(target.len()).copied().unwrap_or(max + 1)
}

/// What to say about a command that names no skill.
fn unknown_command_message(command: &str, suggestions: &[String]) -> String {
    let alternatives = if suggestions.is_empty() {
        "no close match — use a listed slash command or describe the intent in plain language.".to_string()
    } else {
        format!("did you mean {}?", suggestions.iter().map(|s| format!("/{s}")).collect::<Vec<_>>().join(" or "))
    };
    format!("Unknown command \"/{command}\" — {alternatives}")
}

/// The target a message points at, when it points at the selection.
fn target_hint(text: &str, context: Option<&RoutingContext>) -> Option<String> {
    if context.is_some_and(|c| c.has_selection) && mentions_any(text, ANAPHORA) {
        return Some("target = current selection".into());
    }
    None
}

/// Whether a command can be typed after a slash: one lowercase kebab-case token.
fn is_slash_token(command: &str) -> bool {
    !command.is_empty() && command.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Whether a version is `major.minor.patch`.
fn is_semver(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn router() -> SkillRouter {
        SkillRouter::default()
    }

    #[test]
    fn the_registry_is_sound() {
        let issues = router().registry_issues();
        assert!(issues.is_empty(), "registry issues: {issues:?}");
    }

    #[test]
    fn a_leading_command_is_parsed_and_normalized() {
        let c = parse_leading_command("/Bottom_Sheet make a login").expect("a command");
        assert_eq!(c.command, "bottom-sheet");
        assert_eq!(c.rest, "make a login");
        assert!(parse_leading_command("no command here").is_none());
    }

    #[test]
    fn a_slash_command_activates_its_skill() {
        let r = router();
        let route = r.route("/bottom-sheet add a checkout sheet", None);
        assert!(route.explicit.contains(&"prototype.bottom-sheet".to_string()), "{route:?}");
        assert!(route.ordered.contains(&"prototype.bottom-sheet".to_string()));
        assert!(route.unknown_command.is_none());
    }

    #[test]
    fn a_dependency_comes_before_the_skill_that_needs_it() {
        let r = router();
        let route = r.route("/bottom-sheet", None);
        let sheet = route.ordered.iter().position(|id| id == "prototype.bottom-sheet").unwrap_or(usize::MAX);
        let overlay = route.ordered.iter().position(|id| id == "prototype.overlay").unwrap_or(usize::MAX);
        assert!(overlay < sheet, "the overlay must come first: {route:?}");
    }

    #[test]
    fn plain_language_matches_a_skill() {
        let r = router();
        let route = r.route("design a login screen", None);
        assert!(route.matched.contains(&"patterns.login".to_string()), "{route:?}");
    }

    #[test]
    fn a_plain_request_for_a_product_on_the_web_activates_its_skill() {
        // The words a person actually types. "the mobile app" must still choose the mobile skill,
        // so the bare word "app" is an alias rather than a trigger: it would drag this skill into
        // every request that mentions an app of any kind.
        let r = router();
        for text in ["I want to make an e-commerce web app", "i want to make a news platform web", "build me an online store", "design a marketplace"]
        {
            let route = r.route(text, None);
            assert!(route.matched.contains(&"design.product-site".to_string()), "{text}: {route:?}");
        }
        let mobile = r.route("a fitness tracker mobile app with a home screen", None);
        assert!(mobile.matched.contains(&"design.mobile".to_string()), "{mobile:?}");
    }

    #[test]
    fn an_unknown_command_is_reported_with_suggestions() {
        let r = router();
        let route = r.route("/botom-sheet", None);
        assert_eq!(route.unknown_command.as_deref(), Some("botom-sheet"));
        assert!(route.suggestions.contains(&"bottom-sheet".to_string()), "{route:?}");
        assert_eq!(route.issues.len(), 1);
        assert!(route.issues[0].message.contains("did you mean"));
    }

    #[test]
    fn a_reference_without_a_selection_asks_which_element_it_means() {
        let r = router();
        let route = r.route("make it bold", None);
        assert_eq!(route.ambiguity.action, AmbiguityAction::Clarify);
    }

    #[test]
    fn a_request_that_introduces_its_own_target_is_inferred() {
        let r = router();
        let route = r.route("build a pricing page and make it bold", None);
        assert_eq!(route.ambiguity.action, AmbiguityAction::Infer);
    }

    #[test]
    fn a_selection_resolves_a_reference() {
        let r = router();
        let context = RoutingContext { selected_node_types: vec!["FRAME".into()], has_selection: true };
        let route = r.route("make it bold", Some(&context));
        assert_eq!(route.ambiguity.action, AmbiguityAction::Infer);
        assert_eq!(route.hint.as_deref(), Some("target = current selection"));
    }

    #[test]
    fn an_explicit_command_executes() {
        let r = router();
        let route = r.route("/design-system", None);
        assert_eq!(route.ambiguity.action, AmbiguityAction::Execute);
    }

    #[test]
    fn the_context_block_names_the_routing_mode_and_the_skills() {
        let r = router();
        let route = r.route("/bottom-sheet", None);
        let block = r.build_context(&route);
        assert!(block.starts_with(SKILL_CONTEXT_MARKER), "{block}");
        assert!(block.contains("Routing mode: execute"));
        assert!(block.contains("### prototype.bottom-sheet"));
        assert!(block.contains("Requires:"));
    }

    #[test]
    fn a_plain_request_gets_no_block() {
        let r = router();
        let route = r.route("hello", None);
        assert_eq!(r.build_context(&route), "");
    }

    #[test]
    fn the_block_is_removed_from_the_text_it_was_added_to() {
        let r = router();
        let route = r.route("/bottom-sheet", None);
        let text = format!("build me a sheet{}", r.build_context(&route));
        assert_eq!(strip_skill_context(&text), "build me a sheet");
    }

    #[test]
    fn an_unknown_command_still_reaches_the_model() {
        let r = router();
        let route = r.route("/nope", None);
        let block = r.build_context(&route);
        assert!(block.contains("Unknown command"), "{block}");
    }

    #[test]
    fn the_edit_distance_gives_up_past_the_limit() {
        assert_eq!(edit_distance("abc", "abc", 2), 0);
        assert_eq!(edit_distance("abc", "abd", 2), 1);
        assert_eq!(edit_distance("abc", "xyzzy", 2), 3);
    }
}
