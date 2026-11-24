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
    PromptSkill {
        id: "default.emil-animate".into(),
        name: "emil-animate".into(),
        display_name: "Emil Kowalski — Animate".into(),
        description: "Motion decisions: purpose, duration, curves, interruptibility, and exits.".into(),
        version: "1.0.0".into(),
        author: "Emil Kowalski".into(),
        source_url: "https://github.com/emilkowalski/skills".into(),
        tags: ["animation".into(), "motion".into(), "emil".into()].to_vec(),
        is_default: true,
        enabled: false,
        prompt_content: "\
# Building Animations (Emil Kowalski)

- Should this animate at all?
  - 100+ times/day (keyboard shortcuts, tab switches): No animation ever.
  - Tens of times/day: Fast and subtle (100–150ms) or nothing.
  - Occasional (modals, drawers, dialogs): Standard animation (200–300ms).
- Never animate something that shouldn't animate.
- Never use ease-in on entrances. Entrances must use ease-out or spring physics.
- Never use scale(0) on entrances: scale from ~0.95 or 0.9.
- Reduced motion and hover gating must always be respected.
- Cheapest tool that works: simple CSS/layout transitions over complex keyframes."
            .into(),
    },
    PromptSkill {
        id: "default.emil-choreograph".into(),
        name: "emil-choreograph".into(),
        display_name: "Emil Kowalski — Choreography".into(),
        description: "Coordinate multiple moving elements: order, overlap, timing, and stillness.".into(),
        version: "1.0.0".into(),
        author: "Emil Kowalski".into(),
        source_url: "https://github.com/emilkowalski/skills".into(),
        tags: ["choreograph".into(), "stagger".into(), "emil".into()].to_vec(),
        is_default: true,
        enabled: false,
        prompt_content: "\
# Motion Choreography (Emil Kowalski)

- Coordinate elements that move together: order, overlap, timing, and stillness.
- Good choreography is about cutting moving parts and running the remaining ones in parallel.
- Avoid waterfall cascades where the user waits for items one by one.
- Keep stagger steps tight: 20–40ms max between sibling items.
- Structure elements into:
  - Lead: Exactly one element leads the animation.
  - Supporting: Moves with or immediately after the lead.
  - Static: Containers and backgrounds stay still while content enters."
            .into(),
    },
    PromptSkill {
        id: "default.emil-gestures".into(),
        name: "emil-gestures".into(),
        display_name: "Emil Kowalski — Gestures".into(),
        description: "Direct manipulation: drag, swipe, sheets that track 1:1 and stay interruptible.".into(),
        version: "1.0.0".into(),
        author: "Emil Kowalski".into(),
        source_url: "https://github.com/emilkowalski/skills".into(),
        tags: ["gestures".into(), "swipe".into(), "touch".into(), "emil".into()].to_vec(),
        is_default: true,
        enabled: false,
        prompt_content: "\
# Gesture-Driven UI (Emil Kowalski)

- Two phases of every gesture:
  - TRACK phase (pointer down): 1:1 position mapping to the finger. No easing, no spring lag.
  - RELEASE phase (pointer lifted): Physics finishes motion (spring seeded with release velocity).
- Interruptibility is non-negotiable: users can catch an animating sheet or card mid-flight.
- Rubber-band at edges: resistance communicates physical boundaries instead of an abrupt hard stop.
- Bounce belongs ONLY at the end of a physical fling or drag, never on a button press."
            .into(),
    },
    PromptSkill {
        id: "default.emil-apple-design".into(),
        name: "emil-apple-design".into(),
        display_name: "Emil Kowalski — Apple Design".into(),
        description: "Fluid interfaces and physical motion: springs, materials, depth, spatial consistency.".into(),
        version: "1.0.0".into(),
        author: "Emil Kowalski".into(),
        source_url: "https://github.com/emilkowalski/skills".into(),
        tags: ["apple".into(), "fluid".into(), "motion".into(), "emil".into()].to_vec(),
        is_default: true,
        enabled: false,
        prompt_content: "\
# Apple Design Foundations (Emil Kowalski)

- Fluid, continuous motion: direct manipulation, momentum, spring physics, and interruptible transitions.
- Spatial consistency: elements originate from and return to where they live in the visual hierarchy.
- Optical sizing, typographic tracking, and leading that adapt naturally to hierarchy.
- Restraint: motion serves orientation, state clarity, and physical continuity — never gratuitous decoration."
            .into(),
    },
    PromptSkill {
        id: "default.emil-design-eng".into(),
        name: "emil-design-eng".into(),
        display_name: "Emil Kowalski — Design Engineering".into(),
        description: "UI polish, component design, interaction nuances, and invisible craft details.".into(),
        version: "1.0.0".into(),
        author: "Emil Kowalski".into(),
        source_url: "https://github.com/emilkowalski/skills".into(),
        tags: ["design-engineering".into(), "polish".into(), "emil".into()].to_vec(),
        is_default: true,
        enabled: false,
        prompt_content: "\
# Design Engineering (Emil Kowalski)

- Taste is trained, not innate: study and reverse-engineer why exceptional interfaces feel right.
- Unseen details compound: press and hover states, popover origin, tooltip timing, transitions vs keyframes.
- Never let an interaction feel disconnected: feedback is immediate (<100ms) and transitions are purposeful.
- Color and contrast: use subtle borders and elevation instead of heavy shadows; preserve visual hierarchy.
- Layout polish: optical alignment over mechanical centering where typography or asymmetric shapes demand it."
            .into(),
    },
    PromptSkill {
        id: "default.impeccable".into(),
        name: "impeccable".into(),
        display_name: "Impeccable Design Craft & Polish".into(),
        description: "Production-grade UI craft: typography, layout, color hierarchy, and edge cases.".into(),
        version: "1.0.0".into(),
        author: "Impeccable".into(),
        source_url: "https://github.com/pbakaus/impeccable".into(),
        tags: ["craft".into(), "quality".into(), "polish".into(), "ui".into()].to_vec(),
        is_default: true,
        enabled: false,
        prompt_content: "\
# Impeccable Design Craft

- Production-grade UI craft: no hedging, no filler, clear point of view.
- Visual hierarchy: scale ambition to surface mode; clear primary focal point, restrained accents.
- Spacing & Rhythm: strict 4px/8px grid alignment across all components and frames.
- Real content: always use real, contextual product copy; never lorem ipsum or generic filler.
- Edge cases: test and design for long names, narrow widths, empty states, and contrast accessibility."
            .into(),
    },
    PromptSkill {
        id: "default.mobile-native".into(),
        name: "mobile-native".into(),
        display_name: "Mobile Native Polish".into(),
        description: "Mobile web & app feel: touch targets, swipe interactions, viewport resilience.".into(),
        version: "1.0.0".into(),
        author: "Mobile Craft".into(),
        source_url: String::new(),
        tags: ["mobile".into(), "touch".into(), "native".into(), "responsive".into()].to_vec(),
        is_default: true,
        enabled: false,
        prompt_content: "\
# Mobile Native Polish

- Touch targets: minimum 44px on their short side, with ample padding.
- Viewport resilience: design with dynamic status bars, home indicators, and bottom sheets in mind.
- Gestures: swipe-to-dismiss and pull-down sheets follow 1:1 finger tracking with momentum release.
- Tap highlight & hover: eliminate sticky hover states on mobile touch targets.
- Content density: mobile navigation stacks cleanly without truncating vital headlines or CTAs."
            .into(),
    },
    PromptSkill {
        id: "default.apple-design".into(),
        name: "apple-design".into(),
        display_name: "Apple Design (HIG)".into(),
        description: "Review and audit UI against Apple Human Interface Guidelines & Liquid Glass craft.".into(),
        version: "1.0.0".into(),
        author: "dickwu".into(),
        source_url: "https://github.com/dickwu/apple-design-skill".into(),
        tags: ["apple".into(), "hig".into(), "design".into(), "review".into()].to_vec(),
        is_default: true,
        enabled: false,
        prompt_content: "\
# Apple Design (Human Interface Guidelines)

- Ground UI/UX decisions in Apple's Human Interface Guidelines with a distinctive design-craft lens.
- Liquid Glass material: subtle translucent borders, optical contrast, blur layers, and background adaptation.
- Component rigor: conform to platform conventions for toolbars, sheets, sidebars, navigation bars, and tab bars.
- Typography: San Francisco optical sizing, tracking, leading, and deliberate visual hierarchy.
- Spatial consistency: fluid springs, physical continuity, interruptible transitions, and momentum gestures.
- Accessibility & Contrast: text and key icons must meet AA contrast ratios (4.5:1 body, 3:1 large); dark mode must preserve hierarchy without blown-out whites.
- Restraint: eliminate templated filler, maintain clear focal points, and design with deliberate purpose."
            .into(),
    },
    PromptSkill {
        id: "default.improve-ui".into(),
        name: "improve-ui".into(),
        display_name: "UI Polish & Improve (ibelick)".into(),
        description: "Audit UI surfaces against design evidence, token consistency, and craftsmanship.".into(),
        version: "1.0.0".into(),
        author: "ibelick".into(),
        source_url: "https://github.com/ibelick/ui-skills".into(),
        tags: ["ui".into(), "polish".into(), "craft".into(), "design-system".into()].to_vec(),
        is_default: true,
        enabled: false,
        prompt_content: "\
# UI Polish & Improve (ibelick)

- Audit product surfaces against their local design evidence: tokens, variables, themes, primitives, and compositions.
- Strict proof gate: identify real contract violations (typography, color, spacing rhythm, layout, responsive presentation) rather than personal preference.
- Coherence & Craft: eliminate micro-inconsistencies in padding, border radius, elevation, and component variants.
- Never invent arbitrary values when the design system or context already establishes a rhythm.
- Ensure actionable, self-contained implementation plans with exact tokens and reusable primitives."
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
            let is_custom = stored.id.starts_with("custom-");
            match out.iter_mut().find(|s| s.id == stored.id || (!is_custom && s.name == stored.name)) {
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

/// Turn any GitHub repository, folder, or file URL into candidate raw Markdown URLs for SKILL.md and README.md.
pub fn github_skill_candidate_urls(input: &str) -> Vec<String> {
    let raw = input.trim().trim_end_matches('/');
    if raw.is_empty() {
        return Vec::new();
    }
    // Direct raw URL
    if raw.starts_with("https://raw.githubusercontent.com/") || raw.starts_with("http://raw.githubusercontent.com/") {
        return vec![raw.to_string()];
    }

    // Strip github.com prefix if present
    let rest = if let Some(stripped) = raw.strip_prefix("https://github.com/") {
        stripped
    } else if let Some(stripped) = raw.strip_prefix("http://github.com/") {
        stripped
    } else {
        // Not a github.com URL: if it's http/https, try as-is
        if raw.starts_with("http://") || raw.starts_with("https://") {
            return vec![raw.to_string()];
        }
        return Vec::new();
    };

    let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() < 2 {
        return Vec::new();
    }
    let owner = parts[0];
    let repo = parts[1];

    // Check if it's a blob URL: /owner/repo/blob/branch/path...
    if parts.len() >= 5 && parts[2] == "blob" {
        let branch = parts[3];
        let path = parts[4..].join("/");
        return vec![format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{path}")];
    }

    // Check if it's a tree URL: /owner/repo/tree/branch/path...
    if parts.len() >= 4 && parts[2] == "tree" {
        let branch = parts[3];
        let subpath = if parts.len() > 4 { parts[4..].join("/") } else { String::new() };
        if subpath.is_empty() {
            return vec![
                format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}/SKILL.md"),
                format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}/skills/improve-ui/SKILL.md"),
                format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}/README.md"),
            ];
        } else {
            return vec![
                format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{subpath}/SKILL.md"),
                format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{subpath}/README.md"),
                format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{subpath}"),
            ];
        }
    }

    // Repo root: /owner/repo
    vec![
        format!("https://raw.githubusercontent.com/{owner}/{repo}/main/SKILL.md"),
        format!("https://raw.githubusercontent.com/{owner}/{repo}/main/skills/improve-ui/SKILL.md"),
        format!("https://raw.githubusercontent.com/{owner}/{repo}/master/SKILL.md"),
        format!("https://raw.githubusercontent.com/{owner}/{repo}/main/README.md"),
        format!("https://raw.githubusercontent.com/{owner}/{repo}/master/README.md"),
    ]
}

/// Parse a SKILL.md markdown text with YAML frontmatter into (name, description, prompt_content).
pub fn parse_skill_markdown(text: &str, fallback_name: &str) -> (String, String, String) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return (fallback_name.to_string(), String::new(), String::new());
    }

    // Check for YAML frontmatter between `---` and `---`
    if let Some(rest) = trimmed.strip_prefix("---")
        && let Some(end_idx) = rest.find("\n---")
    {
        let frontmatter = &rest[..end_idx];
        let body = rest[end_idx + 4..].trim();

        let mut name = String::new();
        let mut desc = String::new();
        let mut in_desc_block = false;

        for line in frontmatter.lines() {
            let line_trimmed = line.trim();
            if line_trimmed.starts_with("name:") {
                in_desc_block = false;
                if let Some(val) = line_trimmed.strip_prefix("name:") {
                    name = val.trim().trim_matches('"').trim_matches('\'').to_string();
                }
            } else if line_trimmed.starts_with("description:") {
                if let Some(val) = line_trimmed.strip_prefix("description:") {
                    let val = val.trim();
                    if val == ">" || val == "|" {
                        in_desc_block = true;
                    } else {
                        in_desc_block = false;
                        desc = val.trim_matches('"').trim_matches('\'').to_string();
                    }
                }
            } else if in_desc_block {
                if line.starts_with(' ') || line.starts_with('\t') {
                    if !desc.is_empty() {
                        desc.push(' ');
                    }
                    desc.push_str(line.trim());
                } else {
                    in_desc_block = false;
                }
            }
        }

        let final_name = if name.is_empty() { fallback_name.to_string() } else { name };
        return (final_name, desc, body.to_string());
    }

    // No frontmatter: check if first line is a markdown header `# Name`
    let mut name = fallback_name.to_string();
    let desc = String::new();
    for line in trimmed.lines() {
        let line_trimmed = line.trim();
        if let Some(h) = line_trimmed.strip_prefix("# ") {
            name = h.trim().to_string();
            break;
        }
    }

    (name, desc, trimmed.to_string())
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

    #[test]
    fn candidate_urls_generates_raw_github_urls() {
        let urls = github_skill_candidate_urls("https://github.com/dickwu/apple-design-skill");
        assert_eq!(urls[0], "https://raw.githubusercontent.com/dickwu/apple-design-skill/main/SKILL.md");

        let raw = github_skill_candidate_urls("https://raw.githubusercontent.com/owner/repo/main/SKILL.md");
        assert_eq!(raw, vec!["https://raw.githubusercontent.com/owner/repo/main/SKILL.md"]);

        let blob = github_skill_candidate_urls("https://github.com/owner/repo/blob/main/skills/custom/SKILL.md");
        assert_eq!(blob, vec!["https://raw.githubusercontent.com/owner/repo/main/skills/custom/SKILL.md"]);
    }

    #[test]
    fn parse_skill_markdown_extracts_frontmatter_and_body() {
        let md = "---\nname: my-skill\ndescription: A great skill\n---\n# My Skill\n\nRule 1.";
        let (name, desc, body) = parse_skill_markdown(md, "fallback");
        assert_eq!(name, "my-skill");
        assert_eq!(desc, "A great skill");
        assert_eq!(body, "# My Skill\n\nRule 1.");

        let no_fm = "# Direct Skill\n\nJust rules.";
        let (name, desc, body) = parse_skill_markdown(no_fm, "fallback");
        assert_eq!(name, "Direct Skill");
        assert_eq!(desc, "");
        assert_eq!(body, no_fm);
    }
}
