//! Framing: frames survive a stream, oversized frames are refused before their body is read, and every message kind
//! travels on its conversation's lane.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use std::io::ErrorKind;

use futures::io::Cursor;
use ome_net::framing::{Lane, read_frame, write_frame};
use support::{ceremony_frame, ecdh_frame, lobby_frame, sync_frame};

#[tokio::test]
async fn a_frame_round_trips_across_a_stream() {
    let frame = sync_frame(1);
    let mut stream = Cursor::new(Vec::new());

    write_frame(&mut stream, &frame, 1 << 20).await.unwrap();
    stream.set_position(0);

    assert_eq!(read_frame(&mut stream, 1 << 20).await.unwrap(), frame);
}

#[tokio::test]
async fn an_oversized_length_is_refused_before_the_body() {
    // A 4 GiB length prefix with no body: refused as too large, not read as a truncated stream.
    let mut stream = Cursor::new(u32::MAX.to_be_bytes().to_vec());

    let error = read_frame(&mut stream, 1 << 20).await.unwrap_err();

    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert_eq!(
        write_frame(&mut Cursor::new(Vec::new()), &[0; 9], 8)
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidInput
    );
}

#[tokio::test]
async fn a_truncated_frame_is_an_error() {
    let mut bytes = 10u32.to_be_bytes().to_vec();
    bytes.extend_from_slice(&[0; 4]);

    let error = read_frame(&mut Cursor::new(bytes), 1 << 20)
        .await
        .unwrap_err();

    assert_eq!(error.kind(), ErrorKind::UnexpectedEof);
}

#[test]
fn frames_travel_on_their_conversations_lane() {
    assert_eq!(Lane::of(&ecdh_frame(1)), Lane::Ecdh);
    assert_eq!(Lane::of(&ceremony_frame(1)), Lane::Ceremony);
    assert_eq!(Lane::of(&sync_frame(1)), Lane::Sync);
    assert_eq!(Lane::of(&lobby_frame(1)), Lane::Lobby);
    assert_eq!(Lane::of(b"not a message"), Lane::Ceremony);
}
