//! Server-sent events, read off a byte stream.
//!
//! A provider's answer arrives as `data: {…}` lines separated by blank lines, with `event: name`
//! lines naming the kind where the provider has kinds (Anthropic and Google do; OpenAI's is one
//! unnamed kind). What matters here is only the data payloads, in order, and the `data: [DONE]`
//! that ends OpenAI's stream.
//!
//! Everything the reader does is bounded: a line or an event that is too long ends the stream
//! rather than eating memory, because what arrives is untrusted.

use std::io::Read;

/// The most one line may be; a provider line is a JSON object of a few KiB at most.
pub const MAX_LINE: usize = 1 << 20;

/// The most one event's data may be.
///
/// A provider event is a few kilobytes; this cap exists so a hostile stream cannot be read
/// into memory a piece at a time forever.
pub const MAX_DATA: usize = 4 << 20;

/// One event's payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// The kind the provider named, when it named one.
    pub event: Option<String>,
    /// The data payload, with its own newlines kept (Anthropic splits them).
    pub data: String,
}

/// Why reading stopped.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SseError {
    /// The stream said something the reader could not parse.
    #[error("the stream was not readable as server-sent events")]
    BadStream(&'static str),
    /// A line or an event was too long.
    #[error("a line of the stream was too long")]
    TooLarge,
    /// The stream itself could not be read.
    #[error("the stream could not be read")]
    Io,
}

/// Read a stream's events until it ends, calling `on_event` for each.
///
/// Stops early when `stop` says so (the caller's cancel), and reports that with [`SseError::Io`].
pub fn read_events(reader: &mut dyn Read, stop: &dyn Fn() -> bool, on_event: &mut dyn FnMut(Event) -> bool) -> Result<(), SseError> {
    let mut event = Option::<Event>::None;
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    let mut data_bytes = 0usize;
    loop {
        if stop() {
            return Err(SseError::Io);
        }
        match reader.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => return Err(SseError::Io),
        }
        if byte[0] == b'\n' {
            match take_line(&mut line, &mut event, &mut data_bytes, on_event)? {
                Some(()) => return Ok(()),
                None => continue,
            }
        }
        if byte[0] != b'\r' {
            line.push(byte[0]);
            if line.len() > MAX_LINE {
                return Err(SseError::TooLarge);
            }
        }
    }
    if !line.is_empty() {
        take_line(&mut line, &mut event, &mut data_bytes, on_event)?;
    }
    // A stream that ends mid-event still delivers what it said.
    if let Some(e) = event.take()
        && !on_event(e)
    {
        return Ok(());
    }
    Ok(())
}

/// One line read: `Some(())` when the stream ended (`data: [DONE]`), `None` to carry on.
fn take_line(
    line: &mut Vec<u8>,
    event: &mut Option<Event>,
    data_bytes: &mut usize,
    on_event: &mut dyn FnMut(Event) -> bool,
) -> Result<Option<()>, SseError> {
    let text = String::from_utf8_lossy(line).into_owned();
    line.clear();

    if text.is_empty() {
        // A blank line ends an event. One that said nothing (`event: ping` with no data) is
        // not delivered: it carried no payload to parse.
        if let Some(e) = event.take()
            && !e.data.is_empty()
            && !on_event(e)
        {
            return Ok(Some(()));
        }
        *data_bytes = 0;
        return Ok(None);
    }

    let Some((field, value)) = text.split_once(':').map(|(f, v)| (f, v.strip_prefix(' ').unwrap_or(v))) else {
        return Ok(None);
    };
    match field {
        "event" => {
            let e = event.get_or_insert_with(|| Event { event: None, data: String::new() });
            e.event = Some(value.to_string());
        }
        "data" => {
            let e = event.get_or_insert_with(|| Event { event: None, data: String::new() });
            if !e.data.is_empty() {
                e.data.push('\n');
            }
            e.data.push_str(value);
            *data_bytes += value.len();
            if *data_bytes > MAX_DATA {
                return Err(SseError::TooLarge);
            }
            // OpenAI ends its stream with a sentinel.
            if value == "[DONE]" {
                event.take();
                return Ok(Some(()));
            }
        }
        _ => {}
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn events(text: &str) -> Vec<Event> {
        let mut out = Vec::new();
        let mut cursor = Cursor::new(text.as_bytes().to_vec());
        read_events(&mut cursor, &|| false, &mut |e| {
            out.push(e);
            true
        })
        .expect("the stream reads");
        out
    }

    #[test]
    fn an_unnamed_stream_gives_one_event_per_data() {
        let out = events("data: {\"a\":1}\n\ndata: {\"b\":2}\n\ndata: [DONE]\n\n");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].data, "{\"a\":1}");
        assert_eq!(out[1].data, "{\"b\":2}");
        assert!(out.iter().all(|e| e.event.is_none()));
    }

    #[test]
    fn a_named_stream_groups_its_data() {
        let out = events("event: message_start\ndata: {\"x\":1}\n\nevent: content_block_delta\ndata: {\"y\":2}\n\n");
        assert_eq!(out[0].event.as_deref(), Some("message_start"));
        assert_eq!(out[0].data, "{\"x\":1}");
        assert_eq!(out[1].event.as_deref(), Some("content_block_delta"));
    }

    #[test]
    fn a_data_line_split_over_two_lines_is_one_payload_with_a_newline() {
        let out = events("data: {\"a\":\ndata: 1}\n\n");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].data, "{\"a\":\n1}");
    }

    #[test]
    fn windows_line_endings_and_a_trailing_line_are_handled() {
        let out = events("data: {\"a\":1}\r\n\r\ndata: {\"b\":2}\r\n\r\n");
        assert_eq!(out.len(), 2);
        let out = events("data: {\"a\":1}");
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn comments_and_unknown_fields_are_ignored() {
        let out = events(": keep-alive\ndata: {\"a\":1}\n\nid: 5\ndata: {\"b\":2}\n\n");
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn a_done_sentinel_ends_the_stream_without_an_event() {
        let out = events("data: {\"a\":1}\n\ndata: [DONE]\n\ndata: {\"never\":true}\n\n");
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn an_event_without_data_is_not_delivered() {
        let out = events("event: ping\n\n");
        assert!(out.is_empty());
    }

    #[test]
    fn a_line_that_is_too_long_ends_the_stream() {
        let long = "x".repeat(MAX_LINE + 1);
        let mut cursor = Cursor::new(format!("data: {long}\n\n").into_bytes());
        let out = read_events(&mut cursor, &|| false, &mut |_| true);
        assert_eq!(out, Err(SseError::TooLarge));
    }

    #[test]
    fn an_event_that_is_too_much_data_ends_the_stream() {
        let chunk = "x".repeat(MAX_LINE - 10);
        // One event of more than MAX_DATA bytes: five lines just under the line cap.
        let text = format!("data: {chunk}\ndata: {chunk}\ndata: {chunk}\ndata: {chunk}\ndata: {chunk}\n\n");
        let mut cursor = Cursor::new(text.into_bytes());
        let out = read_events(&mut cursor, &|| false, &mut |_| true);
        assert_eq!(out, Err(SseError::TooLarge));
    }

    #[test]
    fn the_caller_can_stop_the_stream() {
        let mut cursor = Cursor::new("data: {\"a\":1}\n\ndata: {\"b\":2}\n\n".as_bytes().to_vec());
        let out = read_events(&mut cursor, &|| false, &mut |_| false);
        // Stopping is reported as an ended read, not an error: the caller asked for it.
        assert!(out.is_ok());
    }
}
