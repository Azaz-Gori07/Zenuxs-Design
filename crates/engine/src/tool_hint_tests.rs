//! What a model gets back when it names the wrong thing. Every answer here is read by a model
//! mid-task: it has to name the tool or command that does the job, because "unknown" sends it
//! looking for another way in and it loses the thread.

use serde_json::json;

use crate::Session;

fn answer(tool: &str, arguments: &serde_json::Value) -> String {
    let entry = crate::chat_tools::table().iter().find(|t| t.name == tool).unwrap_or_else(|| panic!("no tool called {tool}"));
    let mut s = Session::new();
    match entry.run(&mut s, arguments) {
        Ok(v) => v.to_string(),
        Err(e) => e,
    }
}

#[test]
fn a_filter_naming_a_tool_says_it_is_a_tool() {
    // The model read the tool list, then asked the command catalogue for the same name and read
    // "no command matches", concluding the capability did not exist.
    let got = answer("list_commands", &json!({"filter": "source_image"}));
    assert!(got.contains("is a tool, not a command"), "{got}");
    assert!(got.contains("source_image"), "{got}");
}

#[test]
fn a_filter_naming_nothing_still_says_nothing_matches() {
    let got = answer("list_commands", &json!({"filter": "zzz.not.a.thing"}));
    assert!(got.contains("no command matches"), "{got}");
}

#[test]
fn an_invented_image_tool_names_the_one_that_finds_pictures() {
    // The proxy has no image model, so the model that invents a search tool has to be pointed at
    // the free-library one rather than left to try `generate_image` again.
    for invented in ["search_images", "find_photo", "get_image", "fetch_stock_photo"] {
        let got = crate::chat_tools::instead_of(invented);
        assert!(got.contains("source_image"), "{invented}: {got}");
    }
}

#[test]
fn an_invented_lookup_tool_names_the_one_that_reads_the_document() {
    let got = crate::chat_tools::instead_of("read");
    assert!(got.contains("get_node"), "{got}");
    assert!(got.contains("inspect_document"), "{got}");
}

#[test]
fn the_pixel_tools_say_they_need_the_interface() {
    for tool in ["screenshot", "review_design"] {
        let got = answer(tool, &json!({}));
        assert!(got.contains("interface"), "{tool}: {got}");
    }
}
