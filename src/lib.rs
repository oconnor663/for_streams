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
//! That takes three milliseconds and prints `1 101 2 102 3 103`. The behavior here is similar to
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
//! However, importantly, it's _not_ the same as using [`select!`] in a loop like this:
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
//! generally a silent performance bug.
//!
//! And yet, `select!` in a loop gives us an appealing degree of control. Any of the bodies can
//! `break` the loop, for example, which is awkward to replicate with `join!`. So the idea of
//! `for_streams!` is that it's kind of like `select!` in a loop, but specifically for `Stream`s,
//! with fewer footguns and several convenience features.
//!
//! # More interesting examples
//!
//! `continue`, `break`, and `return` are all supported. `continue` skips to the next element of
//! that stream, `break` stops reading from that stream, and `return` ends the whole macro. (The
//! only valid return type is `()`.) This example prints `a2 b1 c1 a4 b2 c2 a6 c3 a8` and then
//! exits.
//!
//! ```rust
//! # use for_streams::for_streams;
//! # use std::time::Duration;
//! # #[tokio::main]
//! # async fn main() {
//! for_streams! {
//!     x in futures::stream::iter(1..1_000_000_000) => {
//!         if x % 2 == 1 {
//!             continue; // Skip the odd elements.
//!         }
//!         print!("a{x} ");
//!         tokio::time::sleep(Duration::from_millis(1)).await;
//!     }
//!     y in futures::stream::iter(1..1_000_000_000) => {
//!         if y > 2 {
//!             break; // Stop this arm after two elements.
//!         }
//!         print!("b{y} ");
//!         tokio::time::sleep(Duration::from_millis(1)).await;
//!     }
//!     z in futures::stream::iter(1..1_000_000_000) => {
//!         if z > 3 {
//!             return; // Stop the whole loop after three elements.
//!         }
//!         print!("c{z} ");
//!         tokio::time::sleep(Duration::from_millis(1)).await;
//!     }
//! }
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
