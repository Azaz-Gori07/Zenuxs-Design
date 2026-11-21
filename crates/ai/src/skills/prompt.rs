//! The prompt skills: what a person writes and switches on.
//!
//! A *semantic* skill (in [`super`]) is structured knowledge the router activates by itself. A
//! *prompt skill* is the other thing: a block of text a person writes, switches on and off, and
//! that is appended to the model's system instructions as it is.
//!
//! The two do not compete. The router decides which semantic skills a message activates; the
//! prompt skills are the person's own standing instructions, always on when they are on.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// A block of standing instructions for the model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PromptSkill {
    /// The skill's id (`custom-…` for one a person wrote).
    pub id: String,
    /// The skill's name, as the person typed it.
    pub name: String,
    /// The name shown in the settings.
    #[serde(default)]
    pub display_name: String,
    /// One line saying what it is for.
    #[serde(default)]
    pub description: String,
    /// The version, so a skill that changes can say so.
    #[serde(default = "default_version")]
    pub version: String,
    /// Who wrote it.
    #[serde(default)]
    pub author: String,
    /// Where it came from.
    #[serde(default)]
    pub source_url: String,
    /// The words it is found by.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Whether it is one of FigCraft's own.
    #[serde(default)]
    pub is_default: bool,
    /// Whether it is switched on.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// The text itself.
    pub prompt_content: String,
}

fn default_version() -> String {
    "1.0.0".into()
}

fn yes() -> bool {
    true
}

/// The prompt skills FigCraft ships with.
pub fn default_prompt_skills() -> &'static [PromptSkill] {
    &PROMPT_SKILLS
}

static PROMPT_SKILLS: std::sync::LazyLock<Vec<PromptSkill>> = std::sync::LazyLock::new(|| {
    vec![
    PromptSkill {
        id: "default.design-craft".into(),
        name: "design-craft".into(),
        display_name: "Design craft".into(),
        description: "Make the design look deliberate: real hierarchy, a restrained palette, and no filler.".into(),
        version: "1.0.0".into(),
        author: "FigCraft".into(),
        source_url: String::new(),
        tags: ["design".into(), "quality".into()].to_vec(),
        is_default: true,
        enabled: false,
        prompt_content: "\
# Design craft

- Look at what is already in the document before adding anything: its type scale, its spacing rhythm, its palette. Reuse them rather than inventing new values.
- One thing carries the emphasis on a screen. If everything is emphasised, nothing is.
- Pick one accent colour and use it for the thing that matters. A second accent halves the first.
- Never leave a placeholder you created yourself unfilled. A grey block you made is unfinished work, not a design choice.
- Write real copy. Specific product language beats placeholder text, and placeholder text in a finished design is a defect.
- Prefer fewer, better elements over many, similar ones. Three card styles in a row is a design system nobody chose."
            .into(),
    },
    PromptSkill {
        id: "default.accessibility".into(),
        name: "accessibility".into(),
        display_name: "Accessibility".into(),
        description: "Contrast, focus, labels and reduced motion: the checks worth making every time.".into(),
        version: "1.0.0".into(),
        author: "FigCraft".into(),
        source_url: String::new(),
        tags: ["accessibility".into()].to_vec(),
        is_default: true,
        enabled: false,
        prompt_content: "\
# Accessibility

- Body text needs 4.5:1 contrast against its background; large text needs 3:1. Check the actual colours, not the intention.
- No state may be signalled by colour alone. Pair a colour change with a shape, a weight or a label.
- Every interactive element needs a visible focus ring and a reachable place in the tab order.
- Labels sit above their fields. A placeholder is not a label.
- Touch targets are at least 44px on their short side.
- Where the platform asks for reduced motion, keep transitions short and subtle rather than large and expressive. Never claim a support that does not exist."
            .into(),
    },
    PromptSkill {
        id: "default.layout".into(),
        name: "layout".into(),
        display_name: "Layout and spacing".into(),
        description: "The spacing scale, the grid, and the rules that keep a layout from drifting.".into(),
        version: "1.0.0".into(),
        author: "FigCraft".into(),
        source_url: String::new(),
        tags: ["layout".into(), "spacing".into()].to_vec(),
        is_default: true,
        enabled: false,
        prompt_content: "\
# Layout and spacing

- Work on a 4px grid: 4, 8, 12, 16, 20, 24, 32, 48. A value off the grid is a value nobody chose.
- Inside a group is tighter than between groups, which is tighter than between sections. Padding is at least the gap of the container it sits in.
- A column of text is given a width and left to fill it. A fixed width on wrapping text guesses the font metrics and spills as soon as they differ.
- A child that should stretch says so (`w=\"fill\"` in auto layout); a child with a fixed size says so too. Never mix the two in one container.
- Nothing escapes the frame that owns it. When an element overlaps its neighbour, the fix is the container's layout, not a nudge to the coordinates.
- Check the arithmetic before building, not after: the vertical budget of the sections against the page height."
            .into(),
    },
    ]
});

/// The prompt skills a person has, with the defaults folded in.
///
/// The defaults are always present, whether or not the stored list has them, so a skill added in
/// a later version appears for everyone. A stored entry overrides the default's own switch and
/// nothing else: the shipped text stays the shipped text.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PromptSkillStore {
    /// The person's own skills, and the switches they set on the defaults.
    #[serde(default)]
    pub skills: Vec<PromptSkill>,
}

impl PromptSkillStore {
    /// Every skill: the defaults with the stored switches applied, then the person's own.
    pub fn all(&self) -> Vec<PromptSkill> {
        let mut out: Vec<PromptSkill> = default_prompt_skills().to_vec();
        for stored in &self.skills {
            match out.iter_mut().find(|s| s.id == stored.id || s.name == stored.name) {
                // A stored entry for a default only moves the switch.
                Some(existing) => existing.enabled = stored.enabled,
                None => {
                    let mut own = stored.clone();
                    own.is_default = false;
                    out.push(own);
                }
            }
        }
        out
    }

    /// The skills that are switched on.
    pub fn active(&self) -> Vec<PromptSkill> {
        self.all().into_iter().filter(|s| s.enabled).collect()
    }

    /// Switch a skill on or off, by id or by name.
    ///
    /// A skill that is not in the store yet is added with the switch set, so switching on a
    /// default that was never touched works.
    pub fn set_enabled(&mut self, id_or_name: &str, enabled: bool) -> bool {
        if let Some(skill) = self.skills.iter_mut().find(|s| s.id == id_or_name || s.name == id_or_name) {
            skill.enabled = enabled;
            return true;
        }
        let Some(default) = default_prompt_skills().iter().find(|s| s.id == id_or_name || s.name == id_or_name) else {
            return false;
        };
        let mut stored = default.clone();
        stored.enabled = enabled;
        self.skills.push(stored);
        true
    }

    /// Add a skill the person wrote.
    pub fn add(&mut self, mut skill: PromptSkill) -> PromptSkill {
        skill.id = if skill.id.trim().is_empty() { format!("custom-{}", skill.name.trim().to_lowercase().replace(' ', "-")) } else { skill.id };
        skill.is_default = false;
        if skill.display_name.trim().is_empty() {
            skill.display_name = skill.name.clone();
        }
        if skill.version.trim().is_empty() {
            skill.version = default_version();
        }
        if skill.author.trim().is_empty() {
            skill.author = "You".into();
        }
        self.skills.retain(|s| s.id != skill.id);
        self.skills.push(skill.clone());
        skill
    }

    /// Remove a skill the person wrote. A default one stays: it is not theirs to remove.
    pub fn remove(&mut self, id_or_name: &str) -> bool {
        let before = self.skills.len();
        self.skills.retain(|s| (s.id != id_or_name && s.name != id_or_name) || s.is_default);
        self.skills.len() != before
    }

    /// Put the defaults back the way they were.
    pub fn reset(&mut self) {
        self.skills.clear();
    }
}

/// How the active prompt skills are presented to the model.
///
/// The preamble matters as much as the blocks: skills that all recommend the same thing must not
/// collapse every design into the same canvas, so the preamble says that variation is part of
/// following them.
pub fn active_prompt_text(skills: &[PromptSkill]) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "# Standing instructions\n\nThese are your standing instructions for every request. Follow them, but treat them as guidance rather than a template: never copy a layout, a palette or a type pairing from an example, and never repeat a composition you have already produced. Every new design must differ from the previous one in structure, palette and type pairing while meeting the same standard.\n",
    );
    for skill in skills {
        out.push_str("\n## ");
        out.push_str(&skill.name);
        if !skill.version.is_empty() {
            out.push_str(&format!(" (v{})", skill.version));
        }
        out.push('\n');
        out.push_str(&skill.prompt_content);
        out.push('\n');
    }
    out
}

/// The ids and names of the skills that are on, for the settings list.
pub fn active_names(skills: &[PromptSkill]) -> BTreeSet<String> {
    skills.iter().filter(|s| s.enabled).map(|s| s.name.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_always_there() {
        let store = PromptSkillStore::default();
        let all = store.all();
        assert_eq!(all.len(), default_prompt_skills().len());
        assert!(all.iter().all(|s| s.is_default));
    }

    #[test]
    fn a_default_is_off_until_it_is_switched_on() {
        let store = PromptSkillStore::default();
        assert!(store.active().is_empty());
    }

    #[test]
    fn switching_a_default_on_is_remembered() {
        let mut store = PromptSkillStore::default();
        assert!(store.set_enabled("design-craft", true));
        assert_eq!(store.active().len(), 1);
        assert_eq!(store.active()[0].id, "default.design-craft");
        // The switch survives a round trip through the stored list.
        assert_eq!(store.skills.len(), 1);
        assert!(store.skills[0].enabled);
    }

    #[test]
    fn a_switch_on_an_unknown_skill_does_nothing() {
        let mut store = PromptSkillStore::default();
        assert!(!store.set_enabled("nope", true));
        assert!(store.active().is_empty());
    }

    #[test]
    fn a_person_can_add_and_remove_a_skill_of_their_own() {
        let mut store = PromptSkillStore::default();
        let skill = store.add(PromptSkill {
            id: String::new(),
            name: "my rules".into(),
            display_name: String::new(),
            description: String::new(),
            version: String::new(),
            author: String::new(),
            source_url: String::new(),
            tags: vec![],
            is_default: false,
            enabled: true,
            prompt_content: "Use the house style.".into(),
        });
        assert_eq!(skill.id, "custom-my-rules");
        assert_eq!(skill.display_name, "my rules");
        assert_eq!(store.active().len(), 1);
        assert!(store.remove(&skill.id));
        assert!(store.active().is_empty());
    }

    #[test]
    fn a_default_can_not_be_removed() {
        let mut store = PromptSkillStore::default();
        assert!(!store.remove("design-craft"));
        assert_eq!(store.all().len(), default_prompt_skills().len());
    }

    #[test]
    fn the_standing_instructions_name_the_skills() {
        let mut store = PromptSkillStore::default();
        store.set_enabled("design-craft", true);
        let text = active_prompt_text(&store.active());
        assert!(text.contains("## design-craft"));
        assert!(text.contains("Standing instructions"));
    }

    #[test]
    fn no_active_skills_means_no_instructions() {
        assert_eq!(active_prompt_text(&[]), "");
    }
}
