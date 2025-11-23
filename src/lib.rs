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
