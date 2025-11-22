use for_streams::for_streams;
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
        // This also indirectly tests that `futures::join!` drops its arguments promptly, as soon
        // as each one of them is finished, rather than all together at the end.
        val in tokio_stream::iter(0..10) => move {
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
            outputs2.push(val);
        }
    }
    assert_eq!(outputs1, (0..10).collect::<Vec<_>>());
    assert_eq!(outputs2, (10..20).collect::<Vec<_>>());
}
