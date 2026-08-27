//! Wire format for the Source RCON protocol.
//!
//! Vanilla parity: `net.minecraft.server.rcon.thread.RconClient.send` and
//! `PktUtils`. Every field is a little-endian `i32`, the payload is a
//! NUL-terminated string, and one more NUL closes the frame. The length prefix
//! counts everything after itself, so it is the payload length plus the two
//! ids, the terminator and the pad.

/// Largest request frame vanilla will read in one go.
///
/// Vanilla parity: `PktUtils.MAX_PACKET_SIZE`.
pub(super) const MAX_PACKET_SIZE: usize = 1460;

/// Bytes a frame carries besides its payload: two ids, a terminator and a pad.
const FRAME_OVERHEAD: usize = 10;

/// Largest response chunk vanilla emits before splitting.
///
/// Vanilla parity: the `4096` of `RconClient.sendCmdResponse`, which is a
/// `String.substring` bound and therefore counts UTF-16 code units.
const MAX_RESPONSE_UNITS: usize = 4096;

/// Client-to-server login attempt. Vanilla `SERVERDATA_AUTH`.
pub(super) const SERVERDATA_AUTH: i32 = 3;
/// Client-to-server command. Vanilla `SERVERDATA_EXECCOMMAND`.
pub(super) const SERVERDATA_EXECCOMMAND: i32 = 2;
/// Server-to-client command output. Vanilla `SERVERDATA_RESPONSE_VALUE`.
pub(super) const SERVERDATA_RESPONSE_VALUE: i32 = 0;
/// Server-to-client login verdict. Vanilla `SERVERDATA_AUTH_RESPONSE`.
pub(super) const SERVERDATA_AUTH_RESPONSE: i32 = 2;
/// Request id a rejected login is answered with. Vanilla `SERVERDATA_AUTH_FAILURE`.
pub(super) const AUTH_FAILURE_REQUEST_ID: i32 = -1;

/// One decoded request frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RconRequest {
    /// Echoed back on every response so a client can pair them up.
    pub(super) request_id: i32,
    /// Which of the `SERVERDATA_*` requests this is.
    pub(super) kind: i32,
    /// The password or the command, depending on `kind`.
    pub(super) body: String,
}

/// Reads the id, kind and body out of a frame's contents.
///
/// `contents` is everything the length prefix covered. A frame shorter than
/// the two ids is malformed; vanilla drops the connection for that, and so
/// does the caller.
pub(super) fn decode_request(contents: &[u8]) -> Option<RconRequest> {
    if contents.len() < 8 {
        return None;
    }
    let request_id = i32::from_le_bytes([contents[0], contents[1], contents[2], contents[3]]);
    let kind = i32::from_le_bytes([contents[4], contents[5], contents[6], contents[7]]);
    // Vanilla parity: `PktUtils.stringFromByteArray` stops at the first NUL and
    // decodes the rest as UTF-8 without validating it.
    let body = &contents[8..];
    let body = body.split(|byte| *byte == 0).next().unwrap_or(body);
    Some(RconRequest {
        request_id,
        kind,
        body: String::from_utf8_lossy(body).into_owned(),
    })
}

/// Builds one response frame.
pub(super) fn encode_response(request_id: i32, kind: i32, payload: &str) -> Vec<u8> {
    let payload = payload.as_bytes();
    let length = payload.len() + FRAME_OVERHEAD;
    let mut frame = Vec::with_capacity(length + 4);
    frame.extend_from_slice(&(length as i32).to_le_bytes());
    frame.extend_from_slice(&request_id.to_le_bytes());
    frame.extend_from_slice(&kind.to_le_bytes());
    frame.extend_from_slice(payload);
    frame.extend_from_slice(&[0, 0]);
    frame
}

/// Splits command output into the chunks one response is sent as.
///
/// Vanilla parity: the `do`/`while` of `RconClient.sendCmdResponse`. The loop
/// body runs before the length is tested, so a command that printed nothing
/// still sends one empty frame -- a client that got no frame at all would wait
/// forever for a reply that already happened.
///
/// Vanilla cuts at a UTF-16 index and will happily halve a surrogate pair.
/// This cuts at the last code-point boundary that fits instead, which no
/// client can tell apart except by not receiving a broken character.
pub(super) fn split_response(response: &str) -> Vec<&str> {
    if response.is_empty() {
        return vec![""];
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    while start < response.len() {
        let mut units = 0;
        let mut end = start;
        for (offset, character) in response[start..].char_indices() {
            let next = units + character.len_utf16();
            if next > MAX_RESPONSE_UNITS {
                break;
            }
            units = next;
            end = start + offset + character.len_utf8();
        }
        chunks.push(&response[start..end]);
        start = end;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_RESPONSE_UNITS, RconRequest, SERVERDATA_AUTH, SERVERDATA_RESPONSE_VALUE,
        decode_request, encode_response, split_response,
    };

    #[test]
    fn a_frame_round_trips_through_the_wire_layout() {
        let frame = encode_response(7, SERVERDATA_RESPONSE_VALUE, "hello");
        assert_eq!(&frame[..4], &15_i32.to_le_bytes(), "length excludes itself");
        assert_eq!(&frame[frame.len() - 2..], &[0, 0], "two NULs close a frame");
        assert_eq!(
            decode_request(&frame[4..]),
            Some(RconRequest {
                request_id: 7,
                kind: SERVERDATA_RESPONSE_VALUE,
                body: "hello".to_owned(),
            })
        );
    }

    #[test]
    fn a_body_stops_at_its_terminator() {
        let mut contents = Vec::new();
        contents.extend_from_slice(&3_i32.to_le_bytes());
        contents.extend_from_slice(&SERVERDATA_AUTH.to_le_bytes());
        contents.extend_from_slice(b"secret\0trailing junk\0");
        let request = decode_request(&contents).expect("a full frame decodes");
        assert_eq!(request.body, "secret");
    }

    #[test]
    fn a_frame_without_room_for_its_ids_is_rejected() {
        assert_eq!(decode_request(&[0, 0, 0, 0, 0, 0, 0]), None);
    }

    #[test]
    fn an_empty_response_is_still_one_frame() {
        assert_eq!(split_response(""), vec![""]);
    }

    #[test]
    fn a_long_response_is_split_and_loses_nothing() {
        let response = "x".repeat(MAX_RESPONSE_UNITS * 2 + 5);
        let chunks = split_response(&response);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), MAX_RESPONSE_UNITS);
        assert_eq!(chunks[1].len(), MAX_RESPONSE_UNITS);
        assert_eq!(chunks[2].len(), 5);
        assert_eq!(chunks.concat(), response);
    }

    #[test]
    fn a_split_never_lands_inside_a_character() {
        // Every one of these is two UTF-16 units, so the cut has to fall one
        // unit short of the bound rather than through the middle of a pair.
        let response = "\u{1F9F1}".repeat(MAX_RESPONSE_UNITS);
        let chunks = split_response(&response);
        assert_eq!(chunks.concat(), response);
        for chunk in &chunks {
            assert!(
                chunk.chars().count() * 2 <= MAX_RESPONSE_UNITS,
                "a chunk may not exceed the vanilla bound"
            );
        }
    }
}
