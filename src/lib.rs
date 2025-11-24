//! The `for_streams!` macro, for driving multiple [`Stream`]s concurrently. `for_streams!` works
//! well with [Tokio](https://tokio.rs/), but it doesn't depend on Tokio.
//!
//! # The simplest case
//!
//! ```rust
//! # use for_streams::for_streams;
//! # use std::time::Duration;
//! # #[tokio::main]
//! # async fn main() {
//! for_streams! {
//!     x in futures::stream::iter(1..=3) => {
//!         tokio::time::sleep(Duration::from_millis(1)).await;
//!         print!("{x} ");
//!     }
//!     y in futures::stream::iter(101..=103) => {
//!         tokio::time::sleep(Duration::from_millis(1)).await;
//!         print!("{y} ");
//!     }
//! }
//! # }
//! ```
//!
//! That takes three milliseconds and prints `1 101 2 102 3 103`. The behavior there is similar to
//! using [`StreamExt::for_each`][for_each] and [`futures::join!`][join] together like this:
//!
//! ```rust
//! # use futures::StreamExt;
//! # use std::time::Duration;
//! # #[tokio::main]
//! # async fn main() {
//! futures::join!(
//!     futures::stream::iter(1..=3).for_each(|x| async move {
//!         tokio::time::sleep(Duration::from_millis(1)).await;
//!         println!("{x}");
//!     }),
//!     futures::stream::iter(101..=103).for_each(|x| async move {
//!         tokio::time::sleep(Duration::from_millis(1)).await;
//!         println!("{x}");
//!     }),
//! );
//! # }
//! ```
//!
//! However, importantly, using [`select!`] in a loop does _not_ behave the same way:
//!
//! ```rust
//! # use futures::StreamExt;
//! # use std::time::Duration;
//! # #[tokio::main]
//! # async fn main() {
//! let mut stream1 = futures::stream::iter(1..=3).fuse();
//! let mut stream2 = futures::stream::iter(101..=103).fuse();
//! loop {
//!     futures::select! {
//!         x = stream1.next() => {
//!             if let Some(x) = x {
//!                 tokio::time::sleep(Duration::from_millis(1)).await;
//!                 println!("{x}");
//!             }
//!         }
//!         y = stream2.next() => {
//!             if let Some(y) = y {
//!                 tokio::time::sleep(Duration::from_millis(1)).await;
//!                 println!("{y}");
//!             }
//!         }
//!         complete => break,
//!     }
//! }
//! # }
//! ```
//!
//! That approach takes _six_ milliseconds, not three. `select!` is [notorious] for cancellation
//! footguns, but this is actually a different problem: the body of a `select!` arm doesn't run
//! concurrently with any other arms (neither their bodies nor their "scrutinees"). Using `select!`
//! in a loop to drive multiple streams is often a mistake, [occasionally a deadlock][deadlock] but
//! frequently a silent performance bug.
//!
//! And yet, `select!` in a loop gives us an appealing degree of control. Any of the bodies can
//! `break` the loop, for example, which is awkward to replicate with `join!`. This is what
//! `for_streams!` is for. It's kind of like `select!` in a loop, but specifically for `Stream`s,
//! with fewer footguns and several convenience features.
//!
//! # More interesting features
//!
//! `continue`, `break`, and `return` are all supported. `continue` skips to the next element of
//! that stream, `break` stops reading from that stream, and `return` ends the whole macro (not the
//! calling function, similar to `return` in an `async` block). The only valid return type is `()`.
//! This example prints `a2 b1 c1 a4 b2 c2 a6 c3 a8` and then exits:
//!
//! ```rust
//! # use for_streams::for_streams;
//! # use std::time::Duration;
//! # #[tokio::main]
//! # async fn main() {
//! for_streams! {
//!     a in futures::stream::iter(1..1_000_000_000) => {
//!         if a % 2 == 1 {
//!             continue; // Skip the odd elements in this arm.
//!         }
//!         print!("a{a} ");
//!         tokio::time::sleep(Duration::from_millis(1)).await;
//!     }
//!     b in futures::stream::iter(1..1_000_000_000) => {
//!         if b > 2 {
//!             break; // Stop this arm after two elements.
//!         }
//!         print!("b{b} ");
//!         tokio::time::sleep(Duration::from_millis(1)).await;
//!     }
//!     c in futures::stream::iter(1..1_000_000_000) => {
//!         if c > 3 {
//!             return; // Stop the whole loop after three elements.
//!         }
//!         print!("c{c} ");
//!         tokio::time::sleep(Duration::from_millis(1)).await;
//!     }
//! }
//! # }
//! ```
//!
//! Sometimes you have a stream that's finite, like a channel that will eventually close, and
//! another streams that's infinite, like a timer that ticks forever. You can use `in background`
//! to tell `for_streams!` not to wait for some arms to finish:
//!
//! ```rust
//! # use for_streams::for_streams;
//! # use std::time::Duration;
//! # #[tokio::main]
//! # async fn main() {
//! use tokio::time::interval;
//! use tokio_stream::wrappers::IntervalStream;
//!
//! let timer = IntervalStream::new(interval(Duration::from_millis(1)));
//! for_streams! {
//!     x in futures::stream::iter(1..10) => {
//!         tokio::time::sleep(Duration::from_millis(1)).await;
//!         println!("{x}");
//!     }
//!     // We'll never reach the end of this `timer` stream, but `in background`
//!     // means we'll exit when the first arm is done, instead of ticking forever.
//!     _ in background timer => {
//!         println!("tick");
//!     }
//! }
//! # }
//! ```
//!
//! The `move` keyword is supported and has the same effect as it would on a lambda or an `async
//! move` block, making the block take ownership of all the values it references. This can be
//! useful if you need a channel writer or a lock guard to drop promptly when one arm is done:
//!
//! ```rust
//! # use for_streams::for_streams;
//! # #[tokio::main]
//! # async fn main() {
//! use tokio::sync::mpsc::channel;
//! use tokio_stream::wrappers::ReceiverStream;
//!
//! // This is a bounded channel, so the sender will block quickly on the
//! // second message if the receiver isn't reading concurrently.
//! let (sender, receiver) = channel::<i32>(1);
//! let mut outputs = Vec::new();
//! for_streams! {
//!     // The `move` keyword makes this arm take ownership of `sender`, which
//!     // means that `sender` drops as soon as this branch is finished. This
//!     // example would deadlock without it.
//!     val in tokio_stream::iter(1..=5) => move {
//!         sender.send(val).await.unwrap();
//!     }
//!     // This arm borrows `outputs` but can't take ownership of it, because
//!     // we use it again below in the assert.
//!     val in ReceiverStream::new(receiver) => {
//!         outputs.push(val);
//!     }
//! }
//! assert_eq!(outputs, vec![1, 2, 3, 4, 5]);
//! # }
//! ```
//!
//! [`Stream`]: https://docs.rs/futures/latest/futures/stream/trait.Stream.html
//! [for_each]: https://docs.rs/futures/latest/futures/stream/trait.StreamExt.html#method.for_each
//! [join]: https://docs.rs/futures/latest/futures/macro.join.html
//! [`select!`]: https://docs.rs/futures/latest/futures/macro.select.html
//! [notorious]: https://sunshowers.io/posts/cancelling-async-rust/
//! [deadlock]: https://rfd.shared.oxide.computer/rfd/0609
use std::pin::Pin;
use std::task::{Context, Poll};

pub use for_streams_impl::for_streams;

// adapted from `futures-lite`
#[doc(hidden)]
pub fn _race<F1: Future, F2: Future>(future1: F1, future2: F2) -> _Race<F1, F2> {
    _Race { future1, future2 }
}

pin_project_lite::pin_project! {
    #[doc(hidden)]
    #[derive(Debug)]
    pub struct _Race<F1, F2> {
        #[pin]
        future1: F1,
        #[pin]
        future2: F2,
    }
}

impl<F1: Future, F2: Future> Future for _Race<F1, F2> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        if this.future1.poll(cx).is_ready() {
            return Poll::Ready(());
        }
        if this.future2.poll(cx).is_ready() {
            return Poll::Ready(());
        }
        Poll::Pending
    }
}
