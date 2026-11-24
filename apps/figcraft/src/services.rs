//! Desktop services: native file dialogs, clipboard, URLs, persisted preferences.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use figcraft_engine::{Prefs, Session};
use figcraft_ui_egui::{DialogFuture, FigApp, Services};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub struct Desktop {
    clipboard: Option<arboard::Clipboard>,
    egui_ctx: Option<egui::Context>,
    /// Where the preferences live ([`config_dir`]).
    dir: Option<PathBuf>,
    pub prefs: Option<Prefs>,
    /// The system's reduce-motion setting (read in the background at startup).
    reduce_motion: Arc<AtomicBool>,
    /// The system's colour scheme ([`watch_system_dark`]): 0 unknown, 1 dark, 2 light.
    dark: Arc<AtomicU8>,
    /// The system's interface language (read at startup).
    language: Option<String>,
    /// Recovery copies of unsaved files (written at once after a panic).
    recovery: crate::recovery::Shared,
    /// The folder of the last file or folder a dialog picked, where the next one starts.
    last_folder: Arc<Mutex<Option<PathBuf>>>,
}

/// The settings folder's name in the platform folder, as it always was (the suite keeps existing
/// folder names), and craft-settings' app id: `FIGCRAFT_CONFIG_DIR` names another folder.
const APP_ID: &str = "FigCraft";

/// Where the preferences, recovery copies and installed plugins live ([`settings_dir`]).
pub fn config_dir() -> Option<PathBuf> {
    settings_dir(&craft_settings::Probe { platform_config_dir: platform_dir(), ..Default::default() })
}

/// The platform's config folder, read from the environment as before: `%APPDATA%` on Windows,
/// so a test's own `APPDATA` keeps the app away from the user's settings.
fn platform_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"));
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    base
}

/// The settings folder by the suite's rules (craft-settings), from what `probe` sees:
/// `FIGCRAFT_CONFIG_DIR` (set but empty: none, nothing read or written); a `portable` file next
/// to the executable (next to the `.app` on macOS), which puts them in `config/` beside it; else
/// `FigCraft` in the platform folder, where they always were. A portable folder without
/// preferences yet starts with a copy of the platform folder's, so an existing copy that becomes
/// portable keeps its settings; the original stays for an installed app. No platform folder: none.
pub(crate) fn settings_dir(probe: &craft_settings::Probe) -> Option<PathBuf> {
    use craft_settings::{Location, Portable, Rule};
    let resolved = craft_settings::paths::resolve(APP_ID, &Portable::DEFAULT, &Location::Auto, probe);
    match (&resolved.rule, &resolved.dir, &probe.platform_config_dir) {
        (Rule::Portable, Some(dir), Some(platform)) => seed(dir, &platform.join(APP_ID)),
        // The current directory instead: settings would follow wherever the app was started.
        (Rule::Fallback, ..) => return None,
        _ => {}
    }
    resolved.dir
}

/// Copy `from`'s preferences into the folder `to` when it has none (owner-only, as written).
fn seed(to: &Path, from: &Path) {
    let (target, source) = (to.join(STATE), from.join(STATE));
    if target.exists() || !source.is_file() {
        return;
    }
    let copied = std::fs::read(&source).and_then(|bytes| std::fs::create_dir_all(to).and_then(|()| write_private(&target, &bytes)));
    match copied {
        Ok(()) => log::info!("preferences: copied {} into the portable folder {}", source.display(), to.display()),
        Err(e) => log::warn!("preferences: couldn't copy them into the portable folder {}: {e}", to.display()),
    }
}

impl Desktop {
    pub fn load(recovery: crate::recovery::Shared) -> Self {
        Self::load_from(config_dir(), recovery)
    }

    /// With the preferences kept in `dir`.
    pub fn load_from(dir: Option<PathBuf>, recovery: crate::recovery::Shared) -> Self {
        let state: Value =
            dir.as_ref().and_then(|d| std::fs::read_to_string(d.join(STATE)).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(json!({}));
        let prefs = state.get("prefs").and_then(|p| serde_json::from_value::<Prefs>(p.clone()).ok()).map(|mut p| {
            // Recent files lived next to the preferences before they moved into them.
            if p.recent_files.is_empty()
                && let Some(old) = state.get("recents").and_then(Value::as_array)
            {
                p.recent_files = old.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
            }
            p.recent_files.truncate(figcraft_engine::cmd::file::MAX_RECENT);
            p
        });
        let reduce_motion = Arc::new(AtomicBool::new(false));
        let flag = reduce_motion.clone();
        std::thread::spawn(move || flag.store(system_reduce_motion(), Ordering::Relaxed));
        let dark = Arc::new(AtomicU8::new(DARK_UNKNOWN));
        Self {
            clipboard: arboard::Clipboard::new().ok(),
            egui_ctx: None,
            dir,
            prefs,
            reduce_motion,
            dark,
            language: system_language(),
            recovery,
            last_folder: Arc::default(),
        }
    }

    /// winit reports no system theme on Linux and the BSDs: there the freedesktop colour scheme
    /// answers instead, read once and then watched in the background, calling `wake` when it
    /// changes (an idle window draws no frames). Elsewhere this does nothing.
    pub fn follow_system_dark(&self, wake: impl Fn() + Send + 'static) {
        #[cfg(all(unix, not(target_os = "macos")))]
        if let Err(e) = std::thread::Builder::new().name("figcraft-theme".into()).spawn({
            let dark = self.dark.clone();
            move || watch_system_dark(&dark, &wake)
        }) {
            log::info!("colour scheme: can't start the watcher: {e}");
        }
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        let _ = wake;
    }
}

/// The system's preferred interface languages, most preferred first, comma-separated
/// ("nl-NL,de-DE,en-US"): Windows' language list, macOS's AppleLanguages, `LANGUAGE` and the
/// locale variables elsewhere. The UI shows the first language in the list that has a catalog.
/// Windows keeps only its first preferred language (and English when it has no catalog): that
/// list is also the input-language list, so a language added for a keyboard (Deutsch on a Polish
/// system) must not decide the interface language.
fn system_language() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        windows::System::UserProfile::GlobalizationPreferences::Languages()
            .and_then(|l| l.GetAt(0))
            .map(|s| s.to_string())
            .inspect_err(|e| log::info!("language: can't read the system languages: {e}"))
            .ok()
    }
    // `defaults` prints a list: ( "es-ES", en ).
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("defaults").args(["read", "-g", "AppleLanguages"]).output().ok()?;
        let list: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .split(['(', ',', ')'])
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Some(list.join(","))
    }
    // gettext's `LANGUAGE` priority list ("fr:de"), then the locale ("fr_FR.UTF-8").
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty() && v != "C" && v != "POSIX");
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"].into_iter().find_map(var);
        let list: Vec<String> = var("LANGUAGE").map(|l| l.replace(':', ",")).into_iter().chain(locale).collect();
        (!list.is_empty()).then(|| list.join(","))
    }
    #[cfg(not(any(windows, unix)))]
    {
        None
    }
}

/// Whether the system's accessibility settings ask for reduced motion: Windows' animation
/// effects (`UISettings.AnimationsEnabled`), macOS's Reduce motion, GNOME's animations.
fn system_reduce_motion() -> bool {
    #[cfg(target_os = "windows")]
    {
        match windows::UI::ViewManagement::UISettings::new().and_then(|s| s.AnimationsEnabled()) {
            Ok(on) => !on,
            Err(e) => {
                log::info!("reduce motion: can't read the animation setting: {e}");
                false
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        command_says("defaults", &["read", "com.apple.universalaccess", "reduceMotion"], "1")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        command_says("gsettings", &["get", "org.gnome.desktop.interface", "enable-animations"], "false")
    }
    #[cfg(not(any(windows, unix)))]
    {
        false
    }
}

/// Whether `program args` runs and prints `want` (trimmed).
#[cfg(unix)]
fn command_says(program: &str, args: &[&str], want: &str) -> bool {
    std::process::Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .is_some_and(|o| String::from_utf8_lossy(&o.stdout).trim() == want)
}

/// [`Desktop::dark`]: the colour scheme is unknown, dark, or light.
const DARK_UNKNOWN: u8 = 0;
const DARK_DARK: u8 = 1;
const DARK_LIGHT: u8 = 2;

#[cfg(all(unix, not(target_os = "macos")))]
const PORTAL: &str = "org.freedesktop.portal.Desktop";
#[cfg(all(unix, not(target_os = "macos")))]
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

/// The desktop's colour scheme through the freedesktop settings portal
/// (`org.freedesktop.appearance` / `color-scheme`: 1 dark, 2 light, 0 no preference, which is
/// light), read once and then watched for changes, each one calling `wake`. Without a session bus
/// or a portal the scheme stays unknown and the UI keeps its light fallback.
#[cfg(all(unix, not(target_os = "macos")))]
fn watch_system_dark(dark: &AtomicU8, wake: &dyn Fn()) {
    use zbus::zvariant::Value;
    const SETTINGS: &str = "org.freedesktop.portal.Settings";
    const APPEARANCE: &str = "org.freedesktop.appearance";
    // `Read` (portals before 1.17, which lack `ReadOne`) wraps the value in one more variant.
    let scheme = |v: &Value| match v {
        Value::U32(x) => *x,
        Value::Value(inner) => match **inner {
            Value::U32(x) => x,
            _ => 0,
        },
        _ => 0,
    };
    let store = |v: u32| {
        let new = match v {
            1 => DARK_DARK,
            2 => DARK_LIGHT,
            _ => DARK_UNKNOWN,
        };
        if dark.swap(new, Ordering::Relaxed) != new {
            wake();
        }
    };
    let Ok(conn) = zbus::blocking::connection::Builder::session().and_then(|b| b.build()) else {
        log::info!("colour scheme: no session bus: the system theme isn't followed");
        return;
    };
    let read = |method: &str| -> zbus::Result<u32> {
        let reply = conn.call_method(Some(PORTAL), PORTAL_PATH, Some(SETTINGS), method, &(APPEARANCE, "color-scheme"))?;
        let body = reply.body();
        Ok(scheme(&body.deserialize::<Value>()?))
    };
    match read("ReadOne").or_else(|_| read("Read")) {
        Ok(v) => store(v),
        Err(e) => log::info!("colour scheme: can't read the system setting: {e}"),
    }
    let watch = || -> zbus::Result<()> {
        let rule =
            zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).sender(PORTAL)?.interface(SETTINGS)?.member("SettingChanged")?.build();
        for msg in zbus::blocking::MessageIterator::for_match_rule(rule, &conn, Some(8))? {
            let msg = msg?;
            let body = msg.body();
            let Ok((namespace, key, value)) = body.deserialize::<(String, String, Value<'_>)>() else { continue };
            if namespace == APPEARANCE && key == "color-scheme" {
                store(scheme(&value));
            }
        }
        Ok(())
    };
    if let Err(e) = watch() {
        log::info!("colour scheme: can't watch the system setting: {e}");
    }
}

/// Show `path` in the file manager, off this thread: the desktop may be slow to answer and the
/// file manager keeps running.
fn reveal(path: &Path) -> std::io::Result<()> {
    let path = path.to_path_buf();
    let show = move || {
        if let Err(e) = show_in_file_manager(&path) {
            log::warn!("show in folder: {}: {e}", path.display());
        }
    };
    std::thread::Builder::new().name("figcraft-reveal".into()).spawn(show).map(|_| ())
}

/// `path` selected in Finder and Explorer, and through the desktop portal where there is one;
/// otherwise its folder. A sandboxed app has to ask the portal: a file picked there lives at
/// `/run/user/…/doc/<id>/<name>`, and opening that folder opens the document itself, in whatever
/// app takes its type (#1701).
fn show_in_file_manager(path: &Path) -> std::io::Result<()> {
    #[cfg(all(unix, not(target_os = "macos")))]
    if shown_by_portal(path, open_directory) {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = std::process::Command::new("open");
        c.arg("-R").arg(path);
        c
    };
    #[cfg(windows)]
    let mut command = {
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(path);
        let mut c = std::process::Command::new("explorer");
        c.arg(arg);
        c
    };
    #[cfg(not(any(target_os = "macos", windows)))]
    let mut command = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path.parent().unwrap_or(path));
        c
    };
    // Waited for so it leaves no zombie.
    command.spawn()?.wait().map(|_| ())
}

/// Whether `portal`, handed `path` itself opened (not its folder), took it.
#[cfg(any(test, all(unix, not(target_os = "macos"))))]
fn shown_by_portal(path: &Path, portal: impl FnOnce(&std::fs::File) -> Result<(), String>) -> bool {
    let shown = std::fs::File::open(path).map_err(|e| e.to_string()).and_then(|f| portal(&f));
    if let Err(e) = &shown {
        log::info!("show in folder: the portal didn't take it ({e}); opening the folder");
    }
    shown.is_ok()
}

/// The portal's `OpenDirectory` (version 3 of `org.freedesktop.portal.OpenURI`): the file manager
/// opens on the folder holding `file`, with it selected.
#[cfg(all(unix, not(target_os = "macos")))]
fn open_directory(file: &std::fs::File) -> Result<(), String> {
    use zbus::zvariant::{Fd, Value};
    const OPEN_URI: &str = "org.freedesktop.portal.OpenURI";
    let conn = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;
    let body = ("", Fd::from(file), std::collections::HashMap::<&str, Value<'_>>::new());
    conn.call_method(Some(PORTAL), PORTAL_PATH, Some(OPEN_URI), "OpenDirectory", &body).map(|_| ()).map_err(|e| e.to_string())
}

/// The preferences' file in the config folder.
const STATE: &str = "state.json";

/// The credentials' file in the config folder: the keys the settings name but never hold.
const CREDENTIALS: &str = "credentials.json";

/// Where the API keys live: a file in the config folder, readable only by its owner.
///
/// A key is never part of the preferences (which are copied around and synced); this file is
/// written owner-only on Unix and holds nothing but the keys themselves. The operating system's
/// own keychain replaces this when a build asks for it.
pub struct DesktopCredentials {
    dir: Option<PathBuf>,
    store: figcraft_ai::credentials::FileCredentialStore,
}

impl DesktopCredentials {
    /// The keys the folder holds, or an empty store.
    pub fn load(dir: Option<&Path>) -> Self {
        let store = dir
            .and_then(|d| std::fs::read_to_string(d.join(CREDENTIALS)).ok())
            .map(|text| figcraft_ai::credentials::FileCredentialStore::from_json(&text))
            .unwrap_or_default();
        Self { dir: dir.map(Path::to_path_buf), store }
    }

    /// Write the keys out, after a change.
    fn persist(&self) {
        let Some(dir) = &self.dir else { return };
        let tmp = dir.join("credentials.json.tmp");
        let written = std::fs::create_dir_all(dir)
            .and_then(|()| write_private(&tmp, self.store.to_json().as_bytes()))
            .and_then(|()| std::fs::rename(&tmp, dir.join(CREDENTIALS)));
        if let Err(e) = written {
            log::warn!("credentials: couldn't save them in {}: {e}", dir.display());
        }
    }
}

/// The AI chat's transport factory: one HTTPS client per request, on its worker thread.
pub struct ChatTransportFactory;

impl figcraft_ai::TransportFactory for ChatTransportFactory {
    fn make(&self, _request: &figcraft_ai::ChatRequest) -> Box<dyn figcraft_ai::Transport> {
        Box::new(figcraft_ai_net::HttpTransport)
    }
}

impl figcraft_ai::credentials::CredentialStore for DesktopCredentials {
    fn status(
        &self,
        reference: &figcraft_ai::credentials::CredentialRef,
    ) -> Result<figcraft_ai::credentials::CredentialStatus, figcraft_ai::credentials::CredentialStoreError> {
        self.store.status(reference)
    }

    fn read(&self, reference: &figcraft_ai::credentials::CredentialRef) -> Result<Option<String>, figcraft_ai::credentials::CredentialStoreError> {
        self.store.read(reference)
    }

    fn write(
        &mut self,
        reference: &figcraft_ai::credentials::CredentialRef,
        value: &str,
    ) -> Result<(), figcraft_ai::credentials::CredentialStoreError> {
        let out = self.store.write(reference, value);
        self.persist();
        out
    }

    fn remove(&mut self, reference: &figcraft_ai::credentials::CredentialRef) -> Result<(), figcraft_ai::credentials::CredentialStoreError> {
        let out = self.store.remove(reference);
        self.persist();
        out
    }
}

const SAVED_SESSIONS_DIR: &str = "SavedSessions";
const SESSION_STATE_FILE: &str = "session.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedTab {
    pub title: String,
    pub path: Option<String>,
    pub draft_file: Option<String>,
    pub is_dirty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSession {
    pub active_tab: usize,
    pub tabs: Vec<SavedTab>,
}

/// Everything quitting keeps, all done when it returns (macOS then ends the process): the
/// preferences in `dir`, this run's recovery copies ([`Recovery::quit`](crate::recovery::Recovery::quit))
/// and the live sessions' ends (one this app hosts tells everyone and stops; a joined one leaves).
pub fn shutdown(app: &mut FigApp, dir: Option<&Path>, recovery: Option<crate::recovery::Recovery>) {
    if let Some(dir) = dir {
        write_prefs(dir, &app.session.prefs);
        save_session_state(dir, &app.session);
    }
    // Before the sessions end: a recovered online file's copy waits for its session's outcome.
    if let Some(r) = recovery {
        r.quit(&app.session, app.ui.quit);
    }
    drop(std::mem::take(&mut app.session.live));
}

/// Persist the open documents and their draft snapshots to `config_dir/SavedSessions/`.
pub fn save_session_state(config_dir: &Path, session: &Session) {
    let session_dir = config_dir.join(SAVED_SESSIONS_DIR);
    if std::fs::create_dir_all(&session_dir).is_err() {
        return;
    }

    let mut tabs = Vec::new();
    let mut active_draft_files = HashSet::new();

    for ds in &session.docs {
        let is_dirty = ds.is_dirty();
        let has_disk_file = ds.path.as_ref().is_some_and(|p| !is_dirty && Path::new(p).exists());

        if has_disk_file {
            tabs.push(SavedTab {
                title: ds.doc.name.clone(),
                path: ds.path.clone(),
                draft_file: None,
                is_dirty: false,
            });
        } else {
            // Untitled canvas or modified file: persist full document draft with versions and branches.
            let draft_name = format!("draft_{}.figcraft", ds.uid);
            let draft_path = session_dir.join(&draft_name);
            let (doc, versions, branches) = figcraft_engine::branches::recovery_copy(session, ds);
            if let Ok(bytes) = figcraft_format::save_bytes_with_history(&doc, None, &versions, &branches)
                && write_private_atomic(&draft_path, &bytes).is_ok()
            {
                active_draft_files.insert(draft_name.clone());
                tabs.push(SavedTab {
                    title: ds.doc.name.clone(),
                    path: ds.path.clone(),
                    draft_file: Some(draft_name),
                    is_dirty,
                });
                continue;
            }
            if let Some(ref p) = ds.path {
                tabs.push(SavedTab {
                    title: ds.doc.name.clone(),
                    path: Some(p.clone()),
                    draft_file: None,
                    is_dirty,
                });
            }
        }
    }

    // Clean up old draft files that are no longer part of active tabs
    if let Ok(entries) = std::fs::read_dir(&session_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().is_some_and(|e| e == "figcraft")
                && let Some(file_name) = p.file_name().and_then(|f| f.to_str())
                && !active_draft_files.contains(file_name)
            {
                let _ = std::fs::remove_file(&p);
            }
        }
    }

    let saved = SavedSession {
        active_tab: session.active,
        tabs,
    };

    if let Ok(json_bytes) = serde_json::to_vec_pretty(&saved) {
        let _ = write_private_atomic(&session_dir.join(SESSION_STATE_FILE), &json_bytes);
    }
}

/// Restore tabs that were open during the last session. Returns true if at least one tab was reopened.
pub fn restore_session_state(config_dir: &Path, session: &mut Session) -> bool {
    let session_dir = config_dir.join(SAVED_SESSIONS_DIR);
    let session_file = session_dir.join(SESSION_STATE_FILE);
    let Ok(data) = std::fs::read(&session_file) else {
        return false;
    };
    let Ok(saved) = serde_json::from_slice::<SavedSession>(&data) else {
        return false;
    };
    if saved.tabs.is_empty() {
        return false;
    }

    let mut restored = 0;
    for tab in saved.tabs {
        if let Some(draft_name) = &tab.draft_file {
            let draft_path = session_dir.join(draft_name);
            if draft_path.exists() {
                let count_before = session.docs.len();
                if session.execute("file.recover", &json!({"path": draft_path.to_string_lossy()})).is_ok()
                    && session.docs.len() > count_before
                {
                    if let Some(ds) = session.docs.last_mut() {
                        ds.path = tab.path.clone();
                        Arc::make_mut(&mut ds.doc).name = tab.title.clone();
                        if !tab.is_dirty {
                            ds.mark_saved();
                        }
                    }
                    restored += 1;
                    continue;
                }
            }
        }

        if let Some(path) = &tab.path
            && Path::new(path).exists()
        {
            let count_before = session.docs.len();
            if session.execute("file.open", &json!({"path": path})).is_ok()
                && session.docs.len() > count_before
            {
                restored += 1;
            }
        }
    }

    if restored > 0 {
        session.active = saved.active_tab.min(session.docs.len().saturating_sub(1));
        true
    } else {
        false
    }
}

/// Write `prefs` to `dir` atomically (a temporary file renamed over the old one: a crash leaves
/// either version whole). They hold online owner tokens, so on Unix only the owner can read them.
pub(crate) fn write_prefs(dir: &Path, prefs: &Prefs) {
    let tmp = dir.join("state.json.tmp");
    let written = std::fs::create_dir_all(dir)
        .and_then(|()| write_private(&tmp, json!({"prefs": prefs}).to_string().as_bytes()))
        .and_then(|()| std::fs::rename(&tmp, dir.join(STATE)));
    if let Err(e) = written {
        log::warn!("preferences: couldn't save them in {}: {e}", dir.display());
    }
}

/// Write `bytes` to `path`, owner read/write only on Unix from before the first byte (a leftover
/// file from a crash is narrowed too: the mode given at open only applies to new files).
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    let mut f = o.open(path)?;
    #[cfg(unix)]
    f.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    f.write_all(bytes)
}

fn write_private_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    write_private(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

thread_local! {
    /// While the main window draws: a dialog parented to it, so a dialog opened from it stays in
    /// front of it (a sheet on macOS). eframe gives other windows no handle: theirs have no parent.
    static PARENT: RefCell<Option<rfd::AsyncFileDialog>> = const { RefCell::new(None) };
}

/// Run the main window's frame `f` with the dialogs it opens parented to `frame`'s window.
pub fn parented<R>(frame: &eframe::Frame, f: impl FnOnce() -> R) -> R {
    PARENT.with(|p| p.replace(Some(rfd::AsyncFileDialog::new().set_parent(frame))));
    let r = f();
    PARENT.with(|p| p.replace(None));
    r
}

/// A dialog with `title`, parented to the main window while it draws.
fn dialog(title: &str) -> rfd::AsyncFileDialog {
    PARENT.with(|p| p.borrow().clone()).unwrap_or_default().set_title(title)
}

/// The dialog opens now; its future gives the path picked.
fn picked(pick: impl Future<Output = Option<rfd::FileHandle>> + 'static) -> DialogFuture {
    Box::pin(async move { pick.await.map(|f| f.path().to_path_buf()) })
}

#[cfg(target_os = "macos")]
const MACOS_PASTEBOARD_PNG: &str = "public.png";

/// How long a clipboard read may wait on the app that copied (Linux and the BSDs).
#[cfg(all(unix, not(target_os = "macos")))]
const CLIPBOARD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Read the clipboard. On Linux and the BSDs the read runs on a helper thread with its own handle
/// and gives up after [`CLIPBOARD_TIMEOUT`]: a Wayland read waits on the app that copied with no
/// timeout of its own, so an app that hangs must not hang the window too.
#[cfg(all(unix, not(target_os = "macos")))]
fn read_clipboard<T: Send + 'static>(_: &mut arboard::Clipboard, read: fn(&mut arboard::Clipboard) -> Option<T>) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(arboard::Clipboard::new().ok().and_then(|mut c| read(&mut c)));
    });
    rx.recv_timeout(CLIPBOARD_TIMEOUT).ok().flatten()
}

/// Read the clipboard (the system's own reads don't wait on the app that copied).
#[cfg(not(all(unix, not(target_os = "macos"))))]
fn read_clipboard<T>(c: &mut arboard::Clipboard, read: fn(&mut arboard::Clipboard) -> Option<T>) -> Option<T> {
    read(c)
}

/// Native Save as filters; other callers keep their existing combined filter.
fn save_filters<'a>(exts: &'a [&'a str]) -> Vec<(&'static str, &'a [&'a str])> {
    if exts == ["figcraft", "fig"] {
        vec![("FigCraft (.figcraft)", &exts[..1]), (figcraft_ui_egui::i18n::tr(".fig (experimental)"), &exts[1..])]
    } else {
        vec![("File", exts)]
    }
}

/// The folder to start the next dialog in once `path` was picked: the file's folder, or the folder
/// picked itself.
pub(crate) fn folder_to_remember(path: &Path, is_folder: bool) -> Option<PathBuf> {
    let folder = if is_folder { Some(path) } else { path.parent() };
    folder.filter(|f| !f.as_os_str().is_empty()).map(Path::to_path_buf)
}

/// The remembered folder, unless it is gone since: then the system's own choice.
pub(crate) fn start_folder(last: Option<PathBuf>) -> Option<PathBuf> {
    last.filter(|f| f.is_dir())
}

impl Desktop {
    /// A dialog that starts in the last folder used. The Linux portal does not remember it between
    /// dialogs as macOS and Windows do.
    fn dialog_in_last_folder(&self, title: &str) -> rfd::AsyncFileDialog {
        let dialog = dialog(title);
        match start_folder(self.last_folder.lock().ok().and_then(|l| l.clone())) {
            Some(folder) => dialog.set_directory(folder),
            None => dialog,
        }
    }
}

/// `pick`, with the folder of what it picks kept in `last` for the next dialog.
pub(crate) fn remembering(last: Arc<Mutex<Option<PathBuf>>>, pick: DialogFuture, is_folder: bool) -> DialogFuture {
    Box::pin(async move {
        let path = pick.await?;
        if let (Some(folder), Ok(mut last)) = (folder_to_remember(&path, is_folder), last.lock()) {
            *last = Some(folder);
        }
        Some(path)
    })
}

impl Services for Desktop {
    fn pick_open(&mut self, title: &str, exts: &[&str]) -> DialogFuture {
        let pick = picked(self.dialog_in_last_folder(title).add_filter("Supported files", exts).pick_file());
        remembering(self.last_folder.clone(), pick, false)
    }
    fn pick_save(&mut self, title: &str, name: &str, exts: &[&str]) -> DialogFuture {
        let mut dialog = self.dialog_in_last_folder(title).set_file_name(name);
        for (label, extensions) in save_filters(exts) {
            dialog = dialog.add_filter(label, extensions);
        }
        let pick = picked(dialog.save_file());
        remembering(self.last_folder.clone(), pick, false)
    }
    fn pick_folder(&mut self, title: &str) -> DialogFuture {
        let pick = picked(self.dialog_in_last_folder(title).pick_folder());
        remembering(self.last_folder.clone(), pick, true)
    }
    fn clipboard_set_text(&mut self, s: &str) {
        if let Some(ctx) = &self.egui_ctx {
            ctx.copy_text(s.to_string());
        }
    }
    fn set_egui_context(&mut self, ctx: &egui::Context) {
        self.egui_ctx = Some(ctx.clone());
    }
    fn clipboard_get_text(&mut self) -> Option<String> {
        read_clipboard(self.clipboard.as_mut()?, |c| c.get_text().ok())
    }
    fn clipboard_get_html(&mut self) -> Option<String> {
        read_clipboard(self.clipboard.as_mut()?, |c| c.get().html().ok())
    }
    fn clipboard_set_png(&mut self, w: u32, h: u32, rgba: &[u8]) {
        if let Some(c) = self.clipboard.as_mut() {
            let _ = c.set_image(arboard::ImageData { width: w as usize, height: h as usize, bytes: std::borrow::Cow::Borrowed(rgba) });
        }
    }
    fn clipboard_get_image(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        let img = read_clipboard(self.clipboard.as_mut()?, |c| c.get_image().ok())?;
        let (w, h) = (u32::try_from(img.width).ok()?, u32::try_from(img.height).ok()?);
        // Refuse absurd sizes (and mismatched buffers) rather than allocate.
        if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 100_000_000 || img.bytes.len() != (w as usize) * (h as usize) * 4 {
            return None;
        }
        Some((w, h, img.bytes.into_owned()))
    }
    #[cfg(target_os = "macos")]
    fn clipboard_get_png(&mut self) -> Option<Vec<u8>> {
        let board = objc2_app_kit::NSPasteboard::generalPasteboard();
        let png = board.dataForType(&objc2_foundation::NSString::from_str(MACOS_PASTEBOARD_PNG))?;
        (png.len() <= figcraft_ui_egui::MAX_CLIPBOARD_PNG_BYTES).then(|| png.to_vec())
    }
    #[cfg(not(target_os = "macos"))]
    fn clipboard_get_png(&mut self) -> Option<Vec<u8>> {
        let (w, h, rgba) = self.clipboard_get_image()?;
        let img = image::RgbaImage::from_raw(w, h, rgba)?;
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).ok()?;
        let bytes = out.into_inner();
        (bytes.len() <= figcraft_ui_egui::MAX_CLIPBOARD_PNG_BYTES).then_some(bytes)
    }
    fn open_url(&mut self, url: &str) {
        let _ = webbrowser::open(url);
    }
    fn reveal_path(&mut self, path: &Path) {
        if let Err(e) = reveal(path) {
            log::warn!("show in folder: {}: {e}", path.display());
        }
    }
    fn save_prefs(&mut self, prefs: &Prefs) {
        if let Some(dir) = &self.dir {
            write_prefs(dir, prefs);
        }
    }
    fn emergency_save(&mut self, session: &Session) {
        if let Some(r) = crate::recovery::lock(&self.recovery).as_mut() {
            r.write(session, true);
            // Earlier runs' copies now covered go, so the next start doesn't open them twice.
            r.settle(session);
        }
    }
    fn reduce_motion(&self) -> bool {
        self.reduce_motion.load(Ordering::Relaxed)
    }
    fn system_dark(&self) -> Option<bool> {
        match self.dark.load(Ordering::Relaxed) {
            DARK_DARK => Some(true),
            DARK_LIGHT => Some(false),
            _ => None,
        }
    }
    fn gamepad_held(&mut self) -> u32 {
        crate::gamepad::held()
    }
    fn system_language(&self) -> Option<String> {
        self.language.clone()
    }
    fn fetch_url(&mut self, url: &str) -> Option<figcraft_ui_egui::TextFetchFuture> {
        let (tx, rx) = std::sync::mpsc::channel();
        let url_owned = url.to_string();
        std::thread::Builder::new().name("figcraft-fetch-skill".into()).spawn(move || {
            let res = figcraft_ai_net::fetch_text(&url_owned);
            let _ = tx.send(res);
        }).ok()?;
        Some(Box::pin(ChannelTextFuture { rx }))
    }
}

struct ChannelTextFuture {
    rx: std::sync::mpsc::Receiver<Result<String, String>>,
}

impl std::future::Future for ChannelTextFuture {
    type Output = Result<String, String>;
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        match self.rx.try_recv() {
            Ok(v) => std::task::Poll::Ready(v),
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                std::task::Poll::Ready(Err("Channel closed".to_string()))
            }
        }
    }
}

/// The window icon from the bundled app icon (if present).
pub fn window_icon() -> Option<egui::IconData> {
    // macOS wants Apple's padded grid; the other platforms the rounded tile edge to edge.
    #[cfg(target_os = "macos")]
    let bytes = include_bytes!("../../../assets/app-icon/figcraft-256.png");
    #[cfg(not(target_os = "macos"))]
    let bytes = include_bytes!("../../../assets/app-icon/hicolor/256x256/apps/ai.storyteller.figcraft.png");
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::IconData { rgba: img.into_raw(), width: w, height: h })
}

#[cfg(test)]
mod reveal_tests {
    use std::cell::Cell;
    use std::io::Read;

    use super::shown_by_portal;

    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("figcraft-reveal-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The portal is handed the file itself: from a sandbox, its folder reads as the document
    /// and the desktop offers to open that instead of showing it (#1701).
    #[test]
    fn the_portal_is_handed_the_file_not_its_folder() {
        let dir = folder("file");
        let path = dir.join("Travel Anxiety App.fig");
        std::fs::write(&path, b"the file").unwrap();
        let portal = |mut file: &std::fs::File| {
            let mut read = String::new();
            file.read_to_string(&mut read).map_err(|e| e.to_string())?;
            assert_eq!(read, "the file");
            Ok(())
        };
        assert!(shown_by_portal(&path, portal));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn without_a_portal_or_the_file_the_folder_opens_instead() {
        let dir = folder("fallback");
        let path = dir.join("a.fig");
        std::fs::write(&path, b"x").unwrap();
        assert!(!shown_by_portal(&path, |_| Err("no such method".into())), "an older portal");
        // A file that's gone never reaches the portal.
        let asked = Cell::new(false);
        let portal = |_: &std::fs::File| {
            asked.set(true);
            Ok(())
        };
        assert!(!shown_by_portal(&dir.join("gone.fig"), portal));
        assert!(!asked.get());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// An empty path opens nothing: no file for the portal, and `xdg-open ""` only prints its usage.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn revealing_runs_off_this_thread_and_falls_back_to_the_folder() {
        let _ = super::show_in_file_manager(std::path::Path::new(""));
        assert!(super::reveal(std::path::Path::new("")).is_ok());
    }
}

#[cfg(test)]
mod save_filter_tests {
    use super::save_filters;

    #[test]
    fn native_and_fig_save_filters_are_separate_with_native_first() {
        let extensions = ["figcraft", "fig"];
        assert_eq!(
            save_filters(&extensions),
            vec![("FigCraft (.figcraft)", &extensions[..1]), (figcraft_ui_egui::i18n::tr(".fig (experimental)"), &extensions[1..])]
        );
    }

    #[test]
    fn other_save_filters_keep_one_combined_group() {
        for extensions in [&["figcraft"][..], &["fig"][..], &["png"][..], &["png", "jpg", "webp"][..], &[][..]] {
            assert_eq!(save_filters(extensions), vec![("File", extensions)]);
        }
    }
}
