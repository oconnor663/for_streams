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
        val in tokio_stream::iter(0..10) => {
            sender1.send(val).await.unwrap();
        }
        val in tokio_stream::iter(10..20) => {
            sender2.send(val).await.unwrap();
        }
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
