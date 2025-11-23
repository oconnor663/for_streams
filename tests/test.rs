use for_streams::for_streams;
use std::task::Poll;
use tokio::sync::mpsc::channel;
use tokio_stream::wrappers::ReceiverStream;

#[tokio::test]
async fn test_channels() {
    // These are bounded channels, so the sender will block quickly on the second message if the
    // receiver isn't reading concurrently. (It would be more fun to use buffer-size-0, i.e.
    // "rendezvous" channels, but Tokio doesn't allow that.)
    let (sender1, receiver1) = channel::<i32>(1);
    let (sender2, receiver2) = channel::<i32>(1);
    let mut outputs1 = Vec::new();
    let mut outputs2 = Vec::new();
    for_streams! {
        // Without the `move` keyword in the sender arms, this test will compile but deadlock. We
        // need to drop the channel senders to allow the channel receivers to read end of stream.
        // This also indirectly tests that `futures::future::Fuse` drops its inner future promptly,
        // as soon as it's ready, rather than when the `Fuse` itself is dropped.
        val in tokio_stream::iter(0..10) => move {
            if val % 2 == 0 {
                continue; // skip the evens
            }
            sender1.send(val).await.unwrap();
        }
        val in tokio_stream::iter(10..20) => move {
            sender2.send(val).await.unwrap();
        }
        // These arms would *not* compile with the `move` keyword, because we reference `outputs1`
        // and `outputs2` again below.
        val in ReceiverStream::new(receiver1) => {
            outputs1.push(val);
        }
        val in ReceiverStream::new(receiver2) => {
            if val % 2 == 1 {
                continue; // skip the odds
            }
            outputs2.push(val);
        }
    }
    assert_eq!(outputs1, vec![1, 3, 5, 7, 9]);
    assert_eq!(outputs2, vec![10, 12, 14, 16, 18]);
}

#[tokio::test]
async fn test_break() {
    let stream1 = futures::stream::iter(0..5);
    let stream2 = futures::stream::iter(0..5);
    let mut outputs1 = Vec::new();
    let mut outputs2 = Vec::new();
    for_streams! {
        val in stream1 => {
            outputs1.push(val);
            if val == 2 {
                // `break` ends this arm, but not the other one.
                break;
            }
        }
        val in stream2 => {
            outputs2.push(val);
        }
    }
    assert_eq!(outputs1, vec![0, 1, 2]);
    assert_eq!(outputs2, vec![0, 1, 2, 3, 4]);
}

#[tokio::test]
async fn test_return() {
    let numbers_stream = futures::stream::iter(0..10);
    let never_stream =
        futures::stream::poll_fn(|_| -> Poll<Option<()>> { std::task::Poll::Pending });
    let mut outputs = Vec::new();
    for_streams! {
        val in numbers_stream => {
            outputs.push(val);
            if val == 5 {
                // Without a `return` here, we deadlock on `never_stream`.
                return;
            }
        }
        _ in never_stream => {}
    }
    assert_eq!(outputs, vec![0, 1, 2, 3, 4, 5]);
}

#[tokio::test]
async fn test_background() {
    let stream1 = futures::stream::iter(0..5);
    // To test a parsing edge case, we'll *name* this stream "background" instead of "stream2",
    // even though it is not in fact the background stream. We need to make sure we can parse that
    // without confusing it for a keyword.
    let background = futures::stream::iter(5..10);
    let mut outputs1 = Vec::new();
    let mut outputs2 = Vec::new();
    let never_stream =
        futures::stream::poll_fn(|_| -> Poll<Option<()>> { std::task::Poll::Pending });
    for_streams! {
        val in stream1 => {
            outputs1.push(val);
        }
        val in background => {
            outputs2.push(val);
        }
        _ in background never_stream => {}
    }
    assert_eq!(outputs1, vec![0, 1, 2, 3, 4]);
    assert_eq!(outputs2, vec![5, 6, 7, 8, 9]);
}
