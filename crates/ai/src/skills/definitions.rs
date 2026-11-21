//! The built-in skills.
//!
//! The skills carry FigCraft's own design vocabulary, and their operations name FigCraft's
//! commands: an agent that activates `design.dashboard` is told to reach for `node.create`,
//! `layout.add` and `command.batch`, not for a tool that does not exist here.
//!
//! The knowledge is guidance for *deciding*, never a template. Two agents given the same skill
//! must not produce the same canvas, so no skill carries a layout to copy.

use std::collections::BTreeMap;

use super::{Skill, SkillCategory, SkillExample};

fn defaults(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

/// The validation rules of a skill.
fn rules(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn examples(pairs: &[(&str, &str)]) -> Vec<SkillExample> {
    pairs.iter().map(|(prompt, intent)| SkillExample { prompt: prompt.to_string(), intent: intent.to_string() }).collect()
}

/// Every skill FigCraft ships with.
pub fn builtin_skills() -> &'static [Skill] {
    &SKILLS
}

static SKILLS: std::sync::LazyLock<Vec<Skill>> = std::sync::LazyLock::new(|| {
    vec![
    // ---------- design ----------
    Skill {
        id: "design.landing-page".into(),
        name: "landing-page".into(),
        category: SkillCategory::Design,
        description: "Marketing landing page structure: hero, proof, features, CTA, footer.".into(),
        version: "1.0.0".into(),
        triggers: ["landing page", "hero section", "marketing page", "splash page"].iter().map(|s| s.to_string()).collect(),
        aliases: ["landing", "hero"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("sections", "hero → social proof → features → CTA → footer"), ("cta", "one primary CTA")]),
        operations: ["node.create", "node.set", "command.batch", "transform.move", "image.place"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "A landing page is a single scrolling page, not an app screen: one clear hero (headline + subcopy + primary CTA), then proof, features, and a closing CTA.",
            "Reuse the document's existing type scale and spacing (4px grid) before inventing new values; when the document already has a design system, extend it (see systems.design-system).",
            "Responsive intent: state desktop and mobile composition (hero stacks, CTA full-width on mobile) as separate frames when asked — responsive layout is not animation.",
            "Buttons get hover/pressed micro-interactions only when requested (see state.interaction); do not add prototype transitions to a static page.",
            "Common mistake: several competing CTAs or an all-text hero. Prefer one primary action and real visual hierarchy.",
        ].join("\n"),
        validation: rules(&[
            "Exactly one dominant hero heading and one primary CTA above the fold.",
            "Spacing follows the document's existing scale; no orphan values off the 4px grid.",
            "Text contrast readable against its background.",
        ]),
        examples: examples(&[("Create a clean analytics landing page.", "static landing page: hero + features + CTA, no prototype")]),
    },
    Skill {
        id: "design.product-site".into(),
        name: "product-site".into(),
        category: SkillCategory::Design,
        description: "A whole product on the web — shop, news, marketplace, SaaS, booking — as a set of real screens with a shared shell.".into(),
        version: "1.0.0".into(),
        triggers: [
            "web app", "webapp", "website", "web site", "e-commerce", "ecommerce", "online store", "store front", "storefront",
            "shop", "marketplace", "news site", "news platform", "magazine", "blog", "portal", "saas", "booking site", "social network",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        aliases: ["app", "site", "product"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[
            ("screens", "home → listing → detail (→ cart/checkout, or article, or dashboard)"),
            ("shell", "one header and one footer shared by every screen"),
            ("width", "1440 desktop web unless the person says mobile"),
        ]),
        operations: ["node.create", "node.set", "command.batch", "image.place", "text.create"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "A product on the web is never one screen. Build the set the brief implies — a home, the listing, the detail — as separate frames side by side, and give them one shell: the same header, the same footer, the same max content width, so they read as one product.",
            "The header is the strongest signal that the screens belong together: wordmark left, navigation centre or left, account and the primary action right. It is the same height and the same padding on every screen.",
            "Content is what makes a screen look real: product names with prices, article headlines with a standfirst and a byline, a date, a category. Never a card holding grey boxes and \"Lorem ipsum\"; the copy is part of the design.",
            "One page is a stack of sections with a rhythm: a hero, then a band of content, then the next band, closing with the footer. Alternate the density so the page has a beat; do not repeat the same card grid five times.",
            "Photography carries a shop and a news site: every image slot gets a real picture, in one `source_image` call for all of them, before the screen is called finished.",
            "A grid of cards is one auto-layout frame wrapping, not eight cards placed at coordinates. Cards in a row share a height; the row's gap is the section's gap.",
            "Common mistake: a beautiful home page and nothing behind it. If the brief names a product, the detail screen is where the design is judged.",
        ]
        .join("\n"),
        validation: rules(&[
            "Every screen shares the same header, footer and content width.",
            "No placeholder copy and no empty image slot on a finished screen.",
            "Sections alternate density; no band repeats the one above it unchanged.",
            "Each screen is its own frame, named for what it shows.",
        ]),
        examples: examples(&[
            ("I want to make an e-commerce web app.", "home + listing + product detail, shared header and footer, real photography"),
            ("I want to make a news platform.", "home with a lead story + section fronts + article page"),
        ]),
    },
    Skill {
        id: "design.dashboard".into(),
        name: "dashboard".into(),
        category: SkillCategory::Design,
        description: "Dashboard/admin structure: nav, KPI grid, content panels, data states.".into(),
        version: "1.0.0".into(),
        triggers: ["dashboard", "analytics dashboard", "admin panel", "admin page"].iter().map(|s| s.to_string()).collect(),
        aliases: ["stats page", "analytics"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("density", "comfortable"), ("structure", "sidebar + KPI grid + detail panels")]),
        operations: ["node.create", "node.set", "command.batch", "transform.move"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "A dashboard is a framed application screen: navigation (sidebar or top bar), a grid of KPI/stat cards, and detail panels (chart areas, tables).",
            "Data surfaces need state thinking: loading (skeleton), empty, and error presentations are states of the same screen (see state.loading) — not three unrelated frames unless asked.",
            "Build skeleton-first: create the frames and the auto layout with `node.create` and `layout.add`, fill them with grey placeholder rectangles, read the result back with `node.get`, fix the sizes in one `command.batch`, then replace the placeholders with real content.",
            "Chart areas are placeholders (rectangles + labels), not fake data visualizations; never fake unsupported capabilities.",
            "Responsive: on mobile the sidebar becomes a drawer (a responsive transformation plus an overlay interaction, see design.responsive and prototype.overlay).",
            "Do not overfit: adapt palette, density, and sections to the request and the existing document; do not always emit the same dark-sidebar layout.",
        ].join("\n"),
        validation: rules(&[
            "One primary nav surface; KPI cards align to a consistent grid.",
            "Loading/empty/error presentations named in the request exist as states or frames.",
        ]),
        examples: examples(&[("Create a clean analytics dashboard.", "framed dashboard: nav, KPI grid, detail panels")]),
    },
    Skill {
        id: "design.mobile".into(),
        name: "mobile".into(),
        category: SkillCategory::Design,
        description: "Mobile app screen conventions: viewport, thumb zone, touch targets.".into(),
        version: "1.0.0".into(),
        triggers: ["mobile app", "mobile screen", "phone screen", "ios app", "android app"].iter().map(|s| s.to_string()).collect(),
        aliases: ["phone", "app screen"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("viewport", "390×844"), ("touchTarget", "44"), ("navigation", "bottom tab bar")]),
        operations: ["node.create", "node.set", "transform.resize", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Mobile screens default to a 390×844 frame. Primary actions live in the thumb zone (the lower two thirds).",
            "Touch targets are at least 44px. A bottom tab bar carries three to five top-level destinations.",
            "Mobile design is layout adaptation — never model it as an animation (see design.responsive).",
            "A mobile screen is a frame in the document like any other; there is no device chassis to draw.",
        ].join("\n"),
        validation: rules(&["Every interactive element is at least 44px on its short side.", "The frame is 390 wide unless the request says otherwise."]),
        examples: examples(&[("Design a mobile settings screen.", "390×844 frame with grouped rows and a bottom tab bar")]),
    },
    Skill {
        id: "design.responsive".into(),
        name: "responsive".into(),
        category: SkillCategory::Responsive,
        description: "Responsive transformation: breakpoints, reflow, sidebar to drawer.".into(),
        version: "1.0.0".into(),
        triggers: ["responsive", "breakpoint", "reflow", "mobile version"].iter().map(|s| s.to_string()).collect(),
        aliases: ["adaptive", "mobile layout", "mobile version"].iter().map(|s| s.to_string()).collect(),
        requires: vec!["design.mobile".into()],
        defaults: defaults(&[("breakpoints", "desktop 1280, tablet 834, mobile 390")]),
        operations: ["node.create", "transform.resize", "command.batch", "layout.add"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Responsive behaviour is layout adaptation, NOT animation. Model it as separate compositions per breakpoint or as flex and grid reflow — never as a transition between breakpoints.",
            "A sidebar becoming a mobile drawer is three things: a responsive transformation (structure), an overlay (the drawer node), and an interaction (open and close).",
            "Name the breakpoints you are designing for before you build; each one is its own frame or its own auto-layout configuration.",
        ].join("\n"),
        validation: rules(&["Each breakpoint named in the request exists in the document.", "No breakpoint change is expressed as a prototype transition."]),
        examples: examples(&[("Make this dashboard responsive.", "a tablet and a mobile composition beside the desktop one")]),
    },
    Skill {
        id: "design.onboarding".into(),
        name: "onboarding".into(),
        category: SkillCategory::Design,
        description: "First-run onboarding flow: sequence, progress, skip, hand-off.".into(),
        version: "1.0.0".into(),
        triggers: ["onboarding", "welcome flow", "first run", "intro screens"].iter().map(|s| s.to_string()).collect(),
        aliases: ["onboarding screens", "first run"].iter().map(|s| s.to_string()).collect(),
        requires: vec!["design.mobile".into(), "prototype.navigation".into()],
        defaults: defaults(&[("steps", "3–4 screens"), ("progress", "dots or a step indicator"), ("last", "a primary Get started button")]),
        operations: ["node.create", "proto.flow.add", "node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Onboarding is a connected sequence of screens (welcome → value props → permissions and setup → start), not one frame.",
            "Every step needs a visible Skip or Next affordance and a progress indication.",
            "Set the first screen as the flow's starting point with `proto.flow.add`, so Present opens on it.",
        ].join("\n"),
        validation: rules(&["Each step has a Next or Skip affordance.", "The first screen is the flow's starting point."]),
        examples: examples(&[("Create a mobile onboarding flow.", "three connected screens with a progress indicator")]),
    },
    // ---------- patterns ----------
    Skill {
        id: "patterns.splash".into(),
        name: "splash".into(),
        category: SkillCategory::Patterns,
        description: "Splash/entry screen: one brand frame that auto-advances into the app.".into(),
        version: "1.0.0".into(),
        triggers: ["splash screen", "launch screen", "entry screen"].iter().map(|s| s.to_string()).collect(),
        aliases: ["splash screen", "entry state"].iter().map(|s| s.to_string()).collect(),
        requires: vec!["prototype.navigation".into()],
        defaults: defaults(&[("content", "brand mark and an optional tagline, nothing else")]),
        operations: ["node.create", "proto.flow.add", "node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "A splash or entry screen is ONE frame — brand mark, optional tagline, nothing else. It hands off automatically: trigger On load, then navigate to the first real screen, usually with a fade.",
            "It is not a step of an onboarding flow: no progress dots, no Skip or Next.",
            "Do not animate the logo by default; motion is requested, not assumed.",
        ].join("\n"),
        validation: rules(&["The splash frame holds only the brand mark and its tagline.", "It navigates on load rather than on a click."]),
        examples: examples(&[("Add a splash screen that leads to the home screen.", "one brand frame that navigates on load")]),
    },
    Skill {
        id: "patterns.login".into(),
        name: "login".into(),
        category: SkillCategory::Patterns,
        description: "Login screen: centered card, labeled fields, submit and error states.".into(),
        version: "1.0.0".into(),
        triggers: ["login screen", "sign in screen", "log in page"].iter().map(|s| s.to_string()).collect(),
        aliases: ["sign-in", "signin", "sign in"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("form", "one column, one primary action"), ("fieldHeight", "44")]),
        operations: ["node.create", "node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Login is one centered card: a heading, the fields, one primary action. One column, one primary CTA — no competing actions.",
            "Field anatomy: label above the input, input about 44px tall, helper or error text below. Labels are mandatory — placeholder-only fields fail accessibility.",
            "The form's states belong to the card (state.loading): idle → submitting → error.",
        ].join("\n"),
        validation: rules(&["Every field has a visible label above it.", "Exactly one primary action on the card."]),
        examples: examples(&[("Design a login screen.", "a centered card with email, password and one primary action")]),
    },
    Skill {
        id: "patterns.signup".into(),
        name: "signup".into(),
        category: SkillCategory::Patterns,
        description: "Sign-up screen: account creation form with confirm, terms, and progress.".into(),
        version: "1.0.0".into(),
        triggers: ["sign up screen", "signup screen", "registration screen", "create account"].iter().map(|s| s.to_string()).collect(),
        aliases: ["register", "sign up"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("form", "one column, one primary action")]),
        operations: ["node.create", "node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Sign-up mirrors login but carries more input: email, password, optional confirm or username, and an accepted-terms line.",
            "Show password requirements as helper text under the field before typing, and inline errors next to the field that failed — never a single error toast for a form.",
        ].join("\n"),
        validation: rules(&["Password requirements are visible before the field is filled.", "Terms acceptance is its own row with its own state."]),
        examples: examples(&[("Create a sign-up screen.", "a form with email, password, terms and one primary action")]),
    },
    Skill {
        id: "patterns.settings".into(),
        name: "settings".into(),
        category: SkillCategory::Patterns,
        description: "Settings screen: grouped rows, nav/detail panes, toggles, destructive actions.".into(),
        version: "1.0.0".into(),
        triggers: ["settings screen", "preferences screen", "settings page"].iter().map(|s| s.to_string()).collect(),
        aliases: ["settings screen", "settings page"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("layout", "grouped rows under section headers")]),
        operations: ["node.create", "node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Settings is a navigation surface: grouped rows under section headers. Desktop uses a nav list plus a detail pane; mobile stacks the list and pushes a detail screen.",
            "Controls must show their current value at rest.",
            "Destructive actions (delete, reset) sit apart from the routine rows, use a danger treatment, and require confirmation.",
        ].join("\n"),
        validation: rules(&["Rows are grouped under headers.", "Destructive actions are visually separate from routine rows."]),
        examples: examples(&[("Design a settings screen.", "grouped rows with a nav list and a detail pane")]),
    },
    Skill {
        id: "patterns.profile".into(),
        name: "profile".into(),
        category: SkillCategory::Patterns,
        description: "Profile screen: identity header, stats, and content sections.".into(),
        version: "1.0.0".into(),
        triggers: ["profile screen", "user profile", "account page"].iter().map(|s| s.to_string()).collect(),
        aliases: ["account screen", "my profile"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[]),
        operations: ["node.create", "node.set", "command.batch", "image.place"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "A profile is an identity header (avatar, name, handle or bio, one primary action) over a small stat row (posts, followers), then tabbed or stacked content sections.",
            "Read and edit are different modes — model them as two states of one screen, not two frames.",
        ].join("\n"),
        validation: rules(&["The identity header carries a name and an avatar.", "Read and edit are states of one screen."]),
        examples: examples(&[("Create a user profile screen.", "identity header, stat row and one content section")]),
    },
    Skill {
        id: "patterns.feed".into(),
        name: "feed".into(),
        category: SkillCategory::Patterns,
        description: "Content feed: stacked post cards, author row, actions, loading and empty states.".into(),
        version: "1.0.0".into(),
        triggers: ["feed screen", "content feed", "news feed", "timeline"].iter().map(|s| s.to_string()).collect(),
        aliases: ["activity feed", "stream"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("pattern", "build one card properly, then repeat it")]),
        operations: ["node.create", "node.set", "command.batch", "image.place"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "A feed is one vertical column of repeating post cards. Build ONE card properly, then repeat the pattern.",
            "There is no runtime infinite scroll; never claim scrolling behaviour the prototype does not have.",
        ].join("\n"),
        validation: rules(&["Every card follows the same structure.", "Loading and empty presentations exist when the request names them."]),
        examples: examples(&[("Create a content feed.", "one card pattern repeated in a column")]),
    },
    Skill {
        id: "patterns.pricing".into(),
        name: "pricing".into(),
        category: SkillCategory::Patterns,
        description: "Pricing table: tiers, highlighted plan, feature comparison, billing toggle.".into(),
        version: "1.0.0".into(),
        triggers: ["pricing page", "pricing table", "plans page", "subscription tiers"].iter().map(|s| s.to_string()).collect(),
        aliases: ["tier table", "plan comparison"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("tiers", "3 or 4"), ("highlighted", "exactly one")]),
        operations: ["node.create", "node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Pricing is a comparison surface: three or four equal tiers. Exactly one tier is visually recommended (accent border or badge, taller or filled) — the rest stay quiet.",
            "Monthly and annual is a state of the same table (state.interaction), not two frames.",
        ].join("\n"),
        validation: rules(&["Exactly one tier is highlighted.", "The tiers share a structure so they compare side by side."]),
        examples: examples(&[("Create a pricing page.", "three tiers with one highlighted plan")]),
    },
    Skill {
        id: "patterns.checkout".into(),
        name: "checkout".into(),
        category: SkillCategory::Patterns,
        description: "Checkout flow: steps, form fields, order summary, loading and error states.".into(),
        version: "1.0.0".into(),
        triggers: ["checkout", "cart", "payment screen", "purchase flow"].iter().map(|s| s.to_string()).collect(),
        aliases: ["shopping cart", "checkout screen"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("shape", "one page with sections, or a short step sequence — pick one")]),
        operations: ["node.create", "node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Checkout is either a short step sequence or one scrolling page with sections — pick one and show progress.",
            "The running order summary (items, subtotal, total) is visible without hunting.",
            "Never invent payment capability: the prototype demonstrates the flow, it does not process anything.",
        ].join("\n"),
        validation: rules(&["The order summary is visible without scrolling past it.", "The flow shows where the person is when it has steps."]),
        examples: examples(&[("Design a checkout screen.", "one page with contact, shipping, payment and an order summary")]),
    },
    Skill {
        id: "patterns.toast".into(),
        name: "toast".into(),
        category: SkillCategory::Patterns,
        description: "Toast/snackbar: transient corner message with optional action and auto-dismiss.".into(),
        version: "1.0.0".into(),
        triggers: ["toast", "snackbar", "flash message", "notification"].iter().map(|s| s.to_string()).collect(),
        aliases: ["status message", "floating message"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("duration", "about 4 seconds"), ("placement", "bottom center on mobile, a corner on desktop")]),
        operations: ["node.create", "node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "A toast is a transient message over content. Model it as an overlay that opens and dismisses: show, then auto-dismiss after about four seconds or on a click, then close.",
            "A toast must never be the only place an error appears — pair it with an inline state.",
        ].join("\n"),
        validation: rules(&["The toast has a dismissal path.", "Errors also appear inline."]),
        examples: examples(&[("Add a toast when the item is saved.", "a transient overlay with an auto-dismiss")]),
    },
    Skill {
        id: "patterns.tooltip".into(),
        name: "tooltip".into(),
        category: SkillCategory::Patterns,
        description: "Tooltip: anchored hover/focus bubble clarifying an icon or control.".into(),
        version: "1.0.0".into(),
        triggers: ["tooltip", "hover hint", "hint bubble"].iter().map(|s| s.to_string()).collect(),
        aliases: ["tool tip", "hint bubble"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("width", "at most 200px"), ("lines", "one or two")]),
        operations: ["node.create", "node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "A tooltip is a small anchored bubble (at most 200px wide, one or two lines), dark surface, light text.",
            "Show it on focus too — hover-only tips are unreachable by keyboard.",
            "A tooltip supplements a label, it does not replace one.",
        ].join("\n"),
        validation: rules(&["The tooltip has a focus trigger as well as hover.", "It does not carry information a label should carry."]),
        examples: examples(&[("Add a tooltip to the export icon.", "a small anchored bubble shown on hover and focus")]),
    },
    Skill {
        id: "patterns.accordion".into(),
        name: "accordion".into(),
        category: SkillCategory::Patterns,
        description: "Accordion/disclosure list: expandable sections with headers and states.".into(),
        version: "1.0.0".into(),
        triggers: ["accordion", "disclosure", "expandable section", "collapsible"].iter().map(|s| s.to_string()).collect(),
        aliases: ["expand collapse", "collapsible list"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("behaviour", "one open at a time, or independent sections — pick one")]),
        operations: ["node.create", "node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "An accordion is a stack of headers (label plus chevron), each owning a content region. Direction, not colour, carries the state.",
            "Choose one behaviour and apply it everywhere: one-open-at-a-time, or independent sections.",
            "Do not nest accordions inside accordions — split them into two lists instead.",
        ].join("\n"),
        validation: rules(&["Every header carries a chevron that matches its state.", "The chosen behaviour is the same for every section."]),
        examples: examples(&[("Create an FAQ accordion.", "a stack of headers, each owning a content region")]),
    },
    Skill {
        id: "patterns.pagination".into(),
        name: "pagination".into(),
        category: SkillCategory::Patterns,
        description: "Pagination: numbered pages, prev/next bounds, current-page state.".into(),
        version: "1.0.0".into(),
        triggers: ["pagination", "page numbers", "pager"].iter().map(|s| s.to_string()).collect(),
        aliases: ["page nav", "paginated list"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("window", "a few page numbers with an ellipsis outside the window")]),
        operations: ["node.create", "node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Pagination is a compact control under a list: a previous arrow, a window of page numbers with an ellipsis outside it, a next arrow, and the current page styled distinctly plus a count (\"Showing 1–20 of 143\").",
            "Touch targets stay at least 44px even though the numbers are small.",
        ].join("\n"),
        validation: rules(&["The current page is visually distinct.", "A count of the shown range is present."]),
        examples: examples(&[("Add pagination under the results list.", "prev, a window of page numbers, next and a count")]),
    },
    Skill {
        id: "patterns.break-ui".into(),
        name: "break-ui".into(),
        category: SkillCategory::Patterns,
        description: "Stress-testing layout and components against edge cases: extreme strings, empty states, missing assets.".into(),
        version: "1.0.0".into(),
        triggers: ["break ui", "stress test", "edge case", "overflow test", "empty state", "boundary check"].iter().map(|s| s.to_string()).collect(),
        aliases: ["stress-test", "edge-case-audit"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("strings", "test German translations and multi-line titles"), ("states", "test 0 items and empty containers")]),
        operations: ["node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Check long copy: German translations, long names, multi-line titles — do buttons wrap or clip?",
            "Check empty states: 0 items in list, missing avatar, network offline.",
            "Check extreme viewports: 320px mobile up to 4K ultrawide.",
            "Check missing media: graceful fallbacks when images fail to load.",
        ].join("\n"),
        validation: rules(&[
            "Containers accommodate multi-line content without clipping.",
            "Empty states and fallbacks are defined.",
        ]),
        examples: examples(&[("Stress test this card layout.", "verify long text wraps and containers do not overflow")]),
    },
    // ---------- prototype ----------
    Skill {
        id: "prototype.navigation".into(),
        name: "prototype".into(),
        category: SkillCategory::Prototype,
        description: "Connect screens into flows: triggers, actions, destinations, flow start.".into(),
        version: "1.0.0".into(),
        triggers: ["prototype", "connect screens", "flow", "link screens"].iter().map(|s| s.to_string()).collect(),
        aliases: ["flow", "navigate", "connect", "navigation"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("transition", "fade, 300ms, ease-out")]),
        operations: ["proto.flow.add", "proto.flow.rename", "node.set", "proto.interaction.remove"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Vocabulary: a trigger is the event (click, press, hover, On load, after delay), an action is what happens (navigate, open overlay, close overlay, set state), a transition is how it moves (instant, fade, slide, spring). Keep them distinct — a component state is not a flow.",
            "\"When it opens, show X\" is trigger load plus navigate.",
            "Modify existing flows in place; do not recreate screens the document already has.",
        ].join("\n"),
        validation: rules(&["Every named destination exists as a node.", "The flow's starting point is set."]),
        examples: examples(&[("Connect the login screen to the dashboard.", "a flow from login to dashboard on a click")]),
    },
    Skill {
        id: "prototype.overlay".into(),
        name: "overlay".into(),
        category: SkillCategory::Prototype,
        description: "Modals, dialogs, popovers, drawers: open and close overlay with backdrop.".into(),
        version: "1.0.0".into(),
        triggers: ["modal", "dialog", "popover", "drawer", "overlay"].iter().map(|s| s.to_string()).collect(),
        aliases: ["modal", "open overlay", "menu overlay"].iter().map(|s| s.to_string()).collect(),
        requires: vec!["prototype.navigation".into()],
        defaults: defaults(&[("backdrop", "0.5 opacity")]),
        operations: ["node.create", "node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "An overlay is a node displayed over the current screen (action openOverlay with the overlay id and a backdrop opacity), not a separate screen. Closing is closeOverlay, which is a no-op when nothing is open, so an outside click can close it safely.",
            "Never fake an overlay with a static rectangle — the runtime renders real overlays in Present.",
        ].join("\n"),
        validation: rules(&["The overlay is opened with an openOverlay action, not a navigate.", "The overlay node exists in the document."]),
        examples: examples(&[("Add a confirmation modal.", "an overlay node opened over the current screen")]),
    },
    Skill {
        id: "prototype.bottom-sheet".into(),
        name: "bottom-sheet".into(),
        category: SkillCategory::Prototype,
        description: "Bottom sheet: slide-up overlay, backdrop, open and close wiring.".into(),
        version: "1.0.0".into(),
        triggers: ["bottom sheet", "sheet", "drawer from bottom"].iter().map(|s| s.to_string()).collect(),
        aliases: ["sheet", "slide up", "bottom drawer"].iter().map(|s| s.to_string()).collect(),
        requires: vec!["prototype.overlay".into(), "motion.slide".into()],
        defaults: defaults(&[("transition", "slide up, 300ms, ease-out")]),
        operations: ["node.create", "node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "A bottom sheet is a frame presented as an overlay entering from the bottom: openOverlay with a slide-up transition, a dimmed backdrop, and a dismissal path (closeOverlay).",
            "\"Checkout should slide up from the bottom\" means the checkout sheet opens as an overlay with a slide transition directed up — not a new screen and not a static frame.",
        ].join("\n"),
        validation: rules(&["The sheet is an overlay, not a screen.", "The slide transition's direction is up."]),
        examples: examples(&[("Add a bottom sheet for checkout.", "an overlay frame that slides up from the bottom")]),
    },
    Skill {
        id: "prototype.carousel".into(),
        name: "carousel".into(),
        category: SkillCategory::Prototype,
        description: "Card carousels and pagers: horizontal advance with slide transitions.".into(),
        version: "1.0.0".into(),
        triggers: ["carousel", "pager", "card slider"].iter().map(|s| s.to_string()).collect(),
        aliases: ["slider", "card deck", "swipe cards"].iter().map(|s| s.to_string()).collect(),
        requires: vec!["prototype.navigation".into()],
        defaults: defaults(&[("transition", "slide left, 300ms")]),
        operations: ["node.create", "node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "A carousel moves horizontally between cards.",
            "There is no swipe or drag trigger; represent \"swipe between cards\" with a click to advance, and record the limitation to the user — never pretend a swipe gesture exists.",
        ].join("\n"),
        validation: rules(&["The advance is a click, not a claimed swipe.", "The slide direction is horizontal."]),
        examples: examples(&[("Make the story cards a carousel.", "click to advance with a slide transition")]),
    },
    // ---------- motion ----------
    Skill {
        id: "motion.fade".into(),
        name: "fade".into(),
        category: SkillCategory::Motion,
        description: "Fade and crossfade transitions: opacity-only change between screens or states.".into(),
        version: "1.0.0".into(),
        triggers: ["fade", "crossfade", "dissolve"].iter().map(|s| s.to_string()).collect(),
        aliases: ["opacity transition"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("duration", "300ms"), ("easing", "ease-out")]),
        operations: ["node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "A fade is an opacity transition. It communicates soft continuity, not spatial movement.",
            "\"Splash fades into login\" is splash On load, then navigate to login with a fade.",
            "Do not animate everything.",
        ].join("\n"),
        validation: rules(&["The transition kind is fade.", "The duration is short enough not to hold the screen."]),
        examples: examples(&[("Fade between the two screens.", "a 300ms fade transition on navigate")]),
    },
    Skill {
        id: "motion.slide".into(),
        name: "slide".into(),
        category: SkillCategory::Motion,
        description: "Slide transitions: directional movement for enters, exits, carousels.".into(),
        version: "1.0.0".into(),
        triggers: ["slide", "swipe transition", "push transition"].iter().map(|s| s.to_string()).collect(),
        aliases: ["slide-up", "slide transition", "page-transition"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("duration", "400ms"), ("easing", "ease-out")]),
        operations: ["node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "A slide is a position or transform transition with an explicit direction. Direction is semantic: sheets enter directed up (from the bottom), carousels advance directed left, exits reverse the entry.",
            "A slide is not a page-layout change: never use motion to model responsive behaviour.",
        ].join("\n"),
        validation: rules(&["The transition has an explicit direction.", "The direction matches where the content comes from."]),
        examples: examples(&[("Slide in the next screen.", "a slide transition directed left")]),
    },
    Skill {
        id: "motion.spring".into(),
        name: "spring".into(),
        category: SkillCategory::Motion,
        description: "Spring and overshoot motion: bounce, snappy, springy feel with sensible easing.".into(),
        version: "1.0.0".into(),
        triggers: ["spring", "bounce", "overshoot", "snappy"].iter().map(|s| s.to_string()).collect(),
        aliases: ["spring physics", "elastic"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("bounce", "0.4")]),
        operations: ["node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Classify the feel: spring or bounce is a spring transition with a bounce (0 none, 1 high); snappy is a shorter duration (about 150–200ms) with a sharp easing; smooth is ease-in-out at 300–400ms.",
            "A spring transition carries a bounce, not an easing and a duration.",
        ].join("\n"),
        validation: rules(&["The transition kind matches the requested feel.", "A spring transition uses a bounce rather than an easing."]),
        examples: examples(&[("Make the card spring in.", "a spring transition with a moderate bounce")]),
    },
    Skill {
        id: "motion.stagger".into(),
        name: "stagger".into(),
        category: SkillCategory::Motion,
        description: "Staggered and sequential appearance: coordinated delays across items.".into(),
        version: "1.0.0".into(),
        triggers: ["stagger", "staggered", "sequential", "cascade"].iter().map(|s| s.to_string()).collect(),
        aliases: ["cascade", "sequence animation"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("step", "60ms"), ("total", "at most 600ms across the group")]),
        operations: ["node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "A stagger is a coordinated delay across several items. Represent it with per-item interactions using an after-delay trigger and an increasing delay.",
            "Keep the total time readable (about 600ms across the group at most).",
            "There is no built-in timeline or keyframe system — do not fake one.",
        ].join("\n"),
        validation: rules(&["The delays increase across the group.", "The whole sequence stays under about 600ms."]),
        examples: examples(&[("Stagger the list items in.", "per-item after-delay triggers with increasing delays")]),
    },
    Skill {
        id: "motion.animate".into(),
        name: "animate".into(),
        category: SkillCategory::Motion,
        description: "Emil Kowalski motion system: build intentional animations that feel physical and purposeful.".into(),
        version: "1.0.0".into(),
        triggers: ["animate", "animation", "motion design", "spring curve", "easing", "motion transition"].iter().map(|s| s.to_string()).collect(),
        aliases: ["motion-design", "motion-principles"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("easing", "ease-out"), ("duration", "200ms")]),
        operations: ["node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Should this animate at all? High-frequency actions (100+/day, like shortcuts or tab switches): NO animation ever.",
            "Tens of times/day actions: fast and subtle (100–150ms) or nothing. Occasional actions (modals, drawers): standard animation (200–300ms).",
            "Never use ease-in on entrances; entrances must use ease-out or spring physics. Never scale from 0.",
            "Reduced motion and hover gating must always be respected.",
            "Cheapest tool that works: simple layout/opacity transitions over complex multi-step animations.",
        ].join("\n"),
        validation: rules(&[
            "Entrances use ease-out or spring, never ease-in.",
            "High-frequency routine actions have no heavy animation.",
        ]),
        examples: examples(&[("Add an intentional motion transition to the dialog.", "200ms ease-out entrance with scale from 0.95")]),
    },
    Skill {
        id: "motion.choreograph".into(),
        name: "choreograph".into(),
        category: SkillCategory::Motion,
        description: "Coordinate multiple moving elements: order, overlap, timing, and stillness.".into(),
        version: "1.0.0".into(),
        triggers: ["choreograph", "choreography", "stagger sequence", "hero entrance", "coordinate motion"].iter().map(|s| s.to_string()).collect(),
        aliases: ["motion-coordination", "choreography"].iter().map(|s| s.to_string()).collect(),
        requires: vec!["motion.animate".into()],
        defaults: defaults(&[("stagger-step", "30ms"), ("max-total", "400ms")]),
        operations: ["node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Good choreography cuts moving parts and runs the remaining ones in parallel.",
            "Never let the user wait for a waterfall cascade: keep stagger step small (20–40ms max).",
            "Structure elements: Lead (only one element leads), Supporting (moves with or slightly after lead), and Static (background and containers remain still).",
        ].join("\n"),
        validation: rules(&[
            "Only one lead moving element.",
            "Stagger delay between items stays under 40ms.",
        ]),
        examples: examples(&[("Coordinate hero entrance motion.", "single lead headline with tightly staggered cards and static shell")]),
    },
    Skill {
        id: "motion.gestures".into(),
        name: "gestures".into(),
        category: SkillCategory::Motion,
        description: "Direct manipulation: drag, swipe, and sheet interactions that track 1:1 and stay interruptible.".into(),
        version: "1.0.0".into(),
        triggers: ["gesture", "gestures", "drag", "swipe", "sheet", "bottom sheet", "pull to refresh", "rubber band", "fling"].iter().map(|s| s.to_string()).collect(),
        aliases: ["gesture-interaction", "direct-manipulation"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("track-phase", "1:1 direct pointer tracking"), ("release-phase", "velocity-seeded spring")]),
        operations: ["node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Two phases of every gesture:",
            "1. TRACK phase: pointer down maps 1:1 to position. No easing, no spring lag. The element tracks the finger directly.",
            "2. RELEASE phase: physics finishes motion (spring seeded with release velocity).",
            "Interruptibility is non-negotiable: users can catch an animating sheet or card mid-flight.",
            "Rubber-band at edges: resistance communicates physical boundaries instead of an abrupt hard stop.",
            "Bounce belongs ONLY at the end of a physical fling or drag, never on a button click.",
        ].join("\n"),
        validation: rules(&[
            "Track phase is 1:1 direct manipulation without lag.",
            "Release phase finishes with velocity physics.",
            "Sheet interactions remain interruptible.",
        ]),
        examples: examples(&[("Build an interactive bottom sheet.", "1:1 finger tracking with rubber-band edge resistance and velocity dismiss")]),
    },
    Skill {
        id: "motion.apple-design".into(),
        name: "apple-design".into(),
        category: SkillCategory::Motion,
        description: "Apple's approach to interface design and fluid physical motion: springs, depth, spatial consistency.".into(),
        version: "1.0.0".into(),
        triggers: ["apple design", "fluid interface", "fluid motion", "spatial consistency", "tactile"].iter().map(|s| s.to_string()).collect(),
        aliases: ["fluid-design", "apple-motion"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("physics", "spring"), ("spatial", "origin-consistent")]),
        operations: ["node.create", "node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Fluid, continuous motion: direct manipulation, momentum, spring physics, and interruptible transitions.",
            "Spatial consistency: elements originate from and return to where they live in the visual hierarchy.",
            "Optical sizing, typographic tracking, and leading that adapt naturally to hierarchy.",
            "Restraint: motion serves orientation, state clarity, and physical continuity — never gratuitous decoration.",
        ].join("\n"),
        validation: rules(&[
            "Motion maintains spatial consistency with element origins.",
            "Spring physics used for interactive transitions.",
        ]),
        examples: examples(&[("Design fluid Apple-style card expansion.", "spring physics with spatial consistency from card origin")]),
    },
    // ---------- state ----------
    Skill {
        id: "state.loading".into(),
        name: "loading".into(),
        category: SkillCategory::State,
        description: "Logical states: loading, success, error, empty — modelled as states, not frames.".into(),
        version: "1.0.0".into(),
        triggers: ["loading state", "empty state", "error state", "skeleton"].iter().map(|s| s.to_string()).collect(),
        aliases: ["states", "progress states", "error", "success"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("states", "loading, empty and error when the request names them")]),
        operations: ["node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "A state is a logical condition the interface can be in. Loading, success, error, empty, disabled, selected and expanded are signals that a request needs explicit state modelling.",
            "Model them on one node (state.loading) and switch them with set-state interactions. Do NOT draw each state as an unrelated static frame.",
            "Keep a loading STATE distinct from a loading ANIMATION.",
        ].join("\n"),
        validation: rules(&["The states live on one node, not on separate frames.", "Every state named in the request exists."]),
        examples: examples(&[("Add a loading state to the results list.", "a loading state on the list node")]),
    },
    Skill {
        id: "state.interaction".into(),
        name: "micro-interaction".into(),
        category: SkillCategory::State,
        description: "Micro-interactions: hover and press feedback, selected visuals, affordance.".into(),
        version: "1.0.0".into(),
        triggers: ["micro-interaction", "hover state", "press state", "selected state", "feedback"].iter().map(|s| s.to_string()).collect(),
        aliases: ["micro interaction", "hover", "pressed", "feedback"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("feedback", "scale 0.98 or a slight opacity change")]),
        operations: ["node.set", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "A micro-interaction is not a page transition. \"Make the button feel clickable\" is hover, press and release triggers plus a small visual delta (scale about 0.98, slight opacity) with quick timing — never a full-screen motion.",
            "A selected tab needs a visually distinct selected state (fill, weight or colour) plus a set-state on click.",
        ].join("\n"),
        validation: rules(&["The feedback is a small visual change, not a transition between screens.", "The selected state is visibly distinct."]),
        examples: examples(&[("Make the button feel clickable.", "hover and press states with a small scale change")]),
    },
    // ---------- systems ----------
    Skill {
        id: "systems.design-system".into(),
        name: "design-system".into(),
        category: SkillCategory::Systems,
        description: "Reuse and extend existing design-system conventions: tokens, scales, variants.".into(),
        version: "1.0.0".into(),
        triggers: ["design system", "tokens", "style guide", "variants"].iter().map(|s| s.to_string()).collect(),
        aliases: ["tokens", "style", "premium"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[]),
        operations: ["variable.list", "style.list", "variable.createCollection", "component.createMultiple", "node.set"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Inspect before inventing: read the document's variables and styles with `variable.list` and `style.list`, then reuse them.",
            "\"More premium\" is a visual-language request: refine hierarchy, spacing rhythm, a restrained palette, and type contrast — do not swap the whole design language.",
            "Project-specific preferences (a 12px radius, say) stay scoped to this document — do not globalize them.",
        ].join("\n"),
        validation: rules(&["New values reuse the document's existing scales where they exist.", "Variables and styles are reused rather than duplicated by name."]),
        examples: examples(&[("Make this more premium.", "refined hierarchy and spacing, not a new visual language")]),
    },
    Skill {
        id: "systems.accessibility".into(),
        name: "accessibility".into(),
        category: SkillCategory::Systems,
        description: "Accessibility intelligence: contrast, focus, labels, reduced motion.".into(),
        version: "1.0.0".into(),
        triggers: ["accessible", "accessibility", "contrast", "wcag", "a11y"].iter().map(|s| s.to_string()).collect(),
        aliases: ["a11y", "contrast", "aria", "focus visible"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[("contrast", "4.5:1 for body text, 3:1 for large text")]),
        operations: ["node.set", "node.get", "command.batch"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "Keyboard: every control reachable in a sensible tab order that follows the visual order, a focus ring visible on each interactive element, no keyboard traps — and Enter or Space on the focused control doing the same thing as a click.",
            "Motion should respect reduced-motion preferences where the runtime supports it; where it does not, prefer short, subtle transitions over large movement — do not claim support that does not exist.",
            "Interactive states must be perceivable without colour alone.",
        ].join("\n"),
        validation: rules(&["Body text meets 4.5:1 against its background.", "No state is signalled by colour alone."]),
        examples: examples(&[("Make this accessible.", "contrast, labels and focus order checked")]),
    },
    Skill {
        id: "systems.document-model".into(),
        name: "document-model".into(),
        category: SkillCategory::Systems,
        description: "How the document is stored: node graph, prototype references, components, plugin data.".into(),
        version: "1.0.0".into(),
        triggers: ["document model", "node graph", "how the file is stored"].iter().map(|s| s.to_string()).collect(),
        aliases: ["node type", "document structure"].iter().map(|s| s.to_string()).collect(),
        requires: vec![],
        defaults: defaults(&[]),
        operations: ["node.get", "node.find", "page.list", "select.children"].iter().map(|s| s.to_string()).collect(),
        knowledge: [
            "The document is a flat graph: every node is keyed by id, hierarchy is expressed with parent and child ids, and pages are canvas nodes holding the top-level frames. Node kinds include FRAME, GROUP, SECTION, COMPONENT, COMPONENT_SET, INSTANCE, TEXT, RECTANGLE, ELLIPSE, LINE, STAR, POLYGON, VECTOR, BOOLEAN_OPERATION, CONNECTOR and SHAPE_WITH_TEXT.",
            "Prototype behaviour is document data on nodes and the root, and every destination, overlay and state it names must be an existing node id.",
            "A component is the master; an instance mirrors its children and carries per-instance overrides; variants are members of a component set.",
            "Selection, viewport zoom, the active tool, hover and undo history are runtime state — never write them into the document.",
        ].join("\n"),
        validation: rules(&["Every referenced id exists in the document.", "No runtime state is written into the document."]),
        examples: examples(&[("How is this file organised?", "the node graph, its pages and its components")]),
    },
    ]
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::SkillRouter;

    #[test]
    fn every_builtin_skill_is_well_formed() {
        let issues = SkillRouter::new(builtin_skills()).registry_issues();
        assert!(issues.is_empty(), "registry issues: {issues:?}");
    }

    #[test]
    fn the_builtin_skills_cover_every_category_but_one() {
        let router = SkillRouter::new(builtin_skills());
        for category in SkillCategory::all() {
            if *category == SkillCategory::Responsive {
                continue;
            }
            assert!(!router.skills_in_category(*category).is_empty(), "{} has no skills", category.id());
        }
    }

    #[test]
    fn emil_kowalski_motion_skills_are_routed() {
        let router = SkillRouter::new(builtin_skills());

        let route = router.route("/animate", None);
        assert!(route.ordered.contains(&"motion.animate".to_string()));

        let route = router.route("/choreograph", None);
        assert!(route.ordered.contains(&"motion.choreograph".to_string()));
        assert!(route.ordered.contains(&"motion.animate".to_string()));

        let route = router.route("/gestures", None);
        assert!(route.ordered.contains(&"motion.gestures".to_string()));

        let route = router.route("/apple-design", None);
        assert!(route.ordered.contains(&"motion.apple-design".to_string()));

        let route = router.route("/break-ui", None);
        assert!(route.ordered.contains(&"patterns.break-ui".to_string()));
    }
}
