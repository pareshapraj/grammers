use grammers_mtproto::{EnqueueError, MTProto};

#[test]
fn ensure_buffer_used_exact_capacity() {
    {
        // Single body (no container)
        let mut mtproto = MTProto::new();

        mtproto
            .enqueue_request(vec![b'H', b'e', b'y', b'!'])
            .unwrap();
        let buffer = mtproto.pop_queue().unwrap();
        assert_eq!(buffer.capacity(), buffer.len());
    }
    {
        // Multiple bodies (using a container)
        let mut mtproto = MTProto::new();

        mtproto
            .enqueue_request(vec![b'H', b'e', b'y', b'!'])
            .unwrap();
        mtproto
            .enqueue_request(vec![b'B', b'y', b'e', b'!'])
            .unwrap();
        let buffer = mtproto.pop_queue().unwrap();
        assert_eq!(buffer.capacity(), buffer.len());
    }
}

fn ensure_buffer_is_message(buffer: &[u8], body: &[u8], seq_no: u8) {
    // buffer[0..8] is the msg_id, based on `SystemTime::now()`
    assert_ne!(&buffer[0..8], [0, 0, 0, 0, 0, 0, 0, 0]);
    // buffer[8..12] is the seq_no, ever-increasing odd number (little endian)
    assert_eq!(&buffer[8..12], [seq_no, 0, 0, 0]);
    // buffer[12..16] is the bytes, the len of the body (little endian)
    assert_eq!(&buffer[12..16], [body.len() as u8, 0, 0, 0]);
    // buffer[16..] is the body, which is padded to 4 bytes
    assert_eq!(&buffer[16..], body);
}

#[test]
fn ensure_correct_single_serialization() {
    let mut mtproto = MTProto::new();

    mtproto
        .enqueue_request(vec![b'H', b'e', b'y', b'!'])
        .unwrap();
    let buffer = mtproto.pop_queue().unwrap();
    ensure_buffer_is_message(&buffer, b"Hey!", 1);
}

#[test]
fn ensure_correct_multi_serialization() {
    let mut mtproto = MTProto::new();

    mtproto
        .enqueue_request(vec![b'H', b'e', b'y', b'!'])
        .unwrap();
    mtproto
        .enqueue_request(vec![b'B', b'y', b'e', b'!'])
        .unwrap();
    let buffer = mtproto.pop_queue().unwrap();

    // buffer[0..8] is the msg_id for the container
    assert_ne!(&buffer[0..8], [0, 0, 0, 0, 0, 0, 0, 0]);
    // buffer[8..12] is the seq_no, maybe-increasing even number.
    // after two messages (1, 3) the next non-content related is 4.
    assert_eq!(&buffer[8..12], [4, 0, 0, 0]);
    // buffer[12..16] is the bytes, the len of the body
    assert_eq!(&buffer[12..16], [48, 0, 0, 0]);

    // buffer[16..20] is the constructor id of the container
    assert_eq!(&buffer[16..20], [0xdc, 0xf8, 0xf1, 0x73]);
    // buffer[20..24] is how many messages are included
    assert_eq!(&buffer[20..24], [2, 0, 0, 0]);

    // buffer[24..44] is an inner message
    ensure_buffer_is_message(&buffer[24..44], b"Hey!", 1);

    // buffer[44..] is the other inner message
    ensure_buffer_is_message(&buffer[44..], b"Bye!", 3);
}

#[test]
fn ensure_correct_single_dependant_serialization() {
    let mut mtproto = MTProto::new();

    let id = mtproto
        .enqueue_request(vec![b'H', b'e', b'y', b'!'])
        .unwrap();
    let first_buffer = mtproto.pop_queue().unwrap();
    mtproto
        .enqueue_sequential_request(vec![b'B', b'y', b'e', b'!'], &id)
        .unwrap();
    let buffer = mtproto.pop_queue().unwrap();
    let invoke_after = {
        let mut tmp = Vec::with_capacity(16);
        tmp.extend(&[0x2d, 0x37, 0x9f, 0xcb]);
        tmp.extend(&first_buffer[0..8]);
        tmp.extend(b"Bye!");
        tmp
    };
    ensure_buffer_is_message(&buffer, &invoke_after, 3);
}

#[test]
fn ensure_correct_multi_dependant_serialization() {
    let mut mtproto = MTProto::new();

    let id = mtproto
        .enqueue_request(vec![b'H', b'e', b'y', b'!'])
        .unwrap();
    mtproto
        .enqueue_sequential_request(vec![b'B', b'y', b'e', b'!'], &id)
        .unwrap();
    let buffer = mtproto.pop_queue().unwrap();

    // buffer[0..8] is the msg_id for the container
    assert_ne!(&buffer[0..8], [0, 0, 0, 0, 0, 0, 0, 0]);
    // buffer[8..12] is the seq_no, maybe-increasing even number.
    // after two messages (1, 3) the next non-content related is 4.
    assert_eq!(&buffer[8..12], [4, 0, 0, 0]);
    // buffer[12..16] is the bytes, the len of the body
    assert_eq!(&buffer[12..16], [60, 0, 0, 0]);

    // buffer[16..20] is the constructor id of the container
    assert_eq!(&buffer[16..20], [0xdc, 0xf8, 0xf1, 0x73]);
    // buffer[20..24] is how many messages are included
    assert_eq!(&buffer[20..24], [2, 0, 0, 0]);

    // buffer[24..44] is an inner message
    ensure_buffer_is_message(&buffer[24..44], b"Hey!", 1);

    // buffer[44..] is the other inner message wrapped in invokeAfterMsg
    let invoke_after = {
        let mut tmp = Vec::with_capacity(16);
        tmp.extend(&[0x2d, 0x37, 0x9f, 0xcb]);
        tmp.extend(&buffer[24..32]);
        tmp.extend(b"Bye!");
        tmp
    };
    ensure_buffer_is_message(&buffer[44..], &invoke_after, 3);
}

#[test]
fn ensure_queue_is_clear() {
    let mut mtproto = MTProto::new();

    assert!(mtproto.pop_queue().is_none());
    let id = mtproto
        .enqueue_request(vec![b'H', b'e', b'y', b'!'])
        .unwrap();

    assert!(mtproto.pop_queue().is_some());
    assert!(mtproto.pop_queue().is_none());
}

#[test]
fn ensure_large_payload_errors() {
    let mut mtproto = MTProto::new();

    assert!(match mtproto.enqueue_request(vec![0; 2 * 1024 * 1024]) {
        Err(EnqueueError::PayloadTooLarge) => true,
        _ => false,
    });

    assert!(mtproto.pop_queue().is_none());

    // Make sure the queue is not in a broken state
    let id = mtproto
        .enqueue_request(vec![b'H', b'e', b'y', b'!'])
        .unwrap();

    assert_eq!(mtproto.pop_queue().unwrap().len(), 20);
}

#[test]
fn ensure_non_padded_payload_errors() {
    let mut mtproto = MTProto::new();

    assert!(match mtproto.enqueue_request(vec![1, 2, 3]) {
        Err(EnqueueError::IncorrectPadding) => true,
        _ => false,
    });

    assert!(mtproto.pop_queue().is_none());

    // Make sure the queue is not in a broken state
    let id = mtproto
        .enqueue_request(vec![b'H', b'e', b'y', b'!'])
        .unwrap();

    assert_eq!(mtproto.pop_queue().unwrap().len(), 20);
}
