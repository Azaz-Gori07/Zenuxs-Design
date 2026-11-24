//! Images and files pasted into the page. eframe's paste event carries only text, so an image or file copied
//! elsewhere (a screenshot, a picture from a web page, an SVG or JSON file) comes in here, and reaches the app as an
//! image paste event or direct import.

use std::cell::RefCell;
use std::sync::Arc;

use egui::{Event, Key, RawInput};
use figcraft_ui_egui::FigApp;
use wasm_bindgen::JsCast as _;
use wasm_bindgen::closure::Closure;

#[derive(Default)]
struct Pasted {
    /// The file in this paste, until the next frame says whether to read it.
    file: Option<web_sys::File>,
    /// Its contents once the browser has handed them over, or why it couldn't.
    read: Option<Result<(String, Vec<u8>), String>>,
    /// For the app to take during this frame only.
    png: Option<Vec<u8>>,
    /// Cached clipboard text from latest paste event.
    text: Option<String>,
}

thread_local! {
    static PASTED: RefCell<Pasted> = RefCell::default();
}

/// Listen to the page's paste events, for the life of the page.
pub fn install(ctx: egui::Context) -> Result<(), wasm_bindgen::JsValue> {
    let document = web_sys::window().and_then(|w| w.document()).ok_or("the page has no document")?;
    let listener = Closure::<dyn Fn(web_sys::ClipboardEvent)>::new(move |e: web_sys::ClipboardEvent| {
        let is_dom_input = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.active_element())
            .is_some_and(|el| el.tag_name() == "INPUT" || el.tag_name() == "TEXTAREA");
        if is_dom_input {
            return;
        }

        if let Some(data) = e.clipboard_data() {
            if let Ok(text) = data.get_data("text") {
                if !text.is_empty() {
                    PASTED.with_borrow_mut(|p| p.text = Some(text));
                }
            }
            if let Some(file) = find_pasted_file(&data) {
                PASTED.with_borrow_mut(|p| p.file = Some(file));
                ctx.request_repaint();
            }
        }
    });
    document.add_event_listener_with_callback("paste", listener.as_ref().unchecked_ref())?;
    listener.forget();
    Ok(())
}

fn find_pasted_file(data: &web_sys::DataTransfer) -> Option<web_sys::File> {
    let files = data.files()?;
    let len = files.length();
    if len == 0 {
        return None;
    }
    for i in 0..len {
        if let Some(f) = files.get(i) {
            let t = f.type_();
            let n = f.name().to_lowercase();
            if t.starts_with("image/")
                || t.contains("svg")
                || t.contains("json")
                || n.ends_with(".png")
                || n.ends_with(".jpg")
                || n.ends_with(".jpeg")
                || n.ends_with(".webp")
                || n.ends_with(".svg")
                || n.ends_with(".json")
            {
                return Some(f);
            }
        }
    }
    files.get(0)
}

/// For the frame about to draw.
pub fn begin_frame(fig: &mut FigApp, raw_input: &mut RawInput, ctx: &egui::Context) {
    let (file, read) = PASTED.with_borrow_mut(|p| {
        p.png = None;
        (p.file.take(), p.read.take())
    });
    let command = |e: &Event| matches!(e, Event::Key { key: Key::V, pressed: true, modifiers, .. } if modifiers.shift || modifiers.alt);
    if let Some(file) = file.filter(|_| !raw_input.events.iter().any(command)) {
        raw_input.events.retain(|e| !matches!(e, Event::Key { key: Key::V, pressed: true, .. }));
        let ctx = ctx.clone();
        let name = file.name();
        wasm_bindgen_futures::spawn_local(async move {
            let read = crate::web::read_file(&file).await.map(|b| (name, b));
            PASTED.with_borrow_mut(|p| p.read = Some(read));
            ctx.request_repaint();
        });
    }
    match read {
        Some(Ok((name, bytes))) => {
            let lower = name.to_lowercase();
            if lower.ends_with(".svg") || lower.ends_with(".json") || lower.ends_with(".figcraft") {
                use base64::Engine as _;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                fig.run_user(
                    "file.import",
                    serde_json::json!({
                        "data": b64,
                        "name": name,
                        "center": true,
                    }),
                );
            } else {
                PASTED.with_borrow_mut(|p| p.png = Some(bytes));
                raw_input.events.push(Event::PasteImage(Arc::default()));
            }
        }
        Some(Err(why)) => fig.toast(why),
        None => {}
    }
}

/// The image pasted this frame.
pub fn take_png() -> Option<Vec<u8>> {
    PASTED.with_borrow_mut(|p| p.png.take())
}

/// Get the latest cached clipboard text from web paste.
pub fn get_clipboard_text() -> Option<String> {
    PASTED.with_borrow(|p| p.text.clone())
}
