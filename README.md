# `for_streams!` [![crates.io](https://img.shields.io/crates/v/for-streams.svg)](https://crates.io/crates/for-streams) [![docs.rs](https://docs.rs/for-streams/badge.svg)](https://docs.rs/for-streams)

**Deprecated:** I don't think this macro is the right approach anymore. See
[`join_me_maybe!`](https://github.com/oconnor663/join_me_maybe/) instead.

The `for_streams!` macro, for driving multiple async [`Stream`]s concurrently. The goal is to
be more convenient and less error-prone than using `select!`-in-a-loop. The stretch goal is to
make the case that most codebases should _ban_ `select!`-in-a-loop.

`for_streams!` works well with [Tokio](https://tokio.rs/), but it doesn't depend on Tokio.

## The simplest case

Here's what it looks like to drive two streams concurrently:

```rust
use for_streams::for_streams;

for_streams! {
    x in futures::stream::iter(1..=3) => {
        sleep(Duration::from_millis(1)).await;
        print!("{x} ");
    }
    y in futures::stream::iter(101..=103) => {
        sleep(Duration::from_millis(1)).await;
        print!("{y} ");
    }
}
```

That takes three milliseconds and prints `1 101 2 102 3 103`. The behavior above is similar to
using [`StreamExt::for_each`][for_each] and [`futures::join!`][join] together like this:

```rust
futures::join!(
    futures::stream::iter(1..=3).for_each(|x| async move {
        sleep(Duration::from_millis(1)).await;
        println!("{x}");
    }),
    futures::stream::iter(101..=103).for_each(|x| async move {
        sleep(Duration::from_millis(1)).await;
        println!("{x}");
    }),
);
```

However, importantly, using [`select!`][futures_select] in a loop does _not_ behave the same
way:

```rust
let mut stream1 = futures::stream::iter(1..=3).fuse();
let mut stream2 = futures::stream::iter(101..=103).fuse();
loop {
    futures::select! {
        x = stream1.next() => {
            if let Some(x) = x {
                sleep(Duration::from_millis(1)).await;
                println!("{x}");
            }
        }
        y = stream2.next() => {
            if let Some(y) = y {
                sleep(Duration::from_millis(1)).await;
                println!("{y}");
            }
        }
        complete => break,
    }
}
```

`select!`-in-a-loop takes _six_ milliseconds, not three. `select!` is
[notorious][cancelling_async_rust] for cancellation footguns, but this is actually a different
problem: the body of a `select!` arm doesn't run concurrently with any other arms (neither
their futures nor their bodies). Using `select!`-in-a-loop is often a mistake, [occasionally a
deadlock][deadlock] but frequently a silent performance bug.

And yet, it does give us a lot of control. Any of the arms can `break` the loop, for example,
or even `return` from the enclosing function. Replicating that sort of control flow with
`join!` is awkward. This is what `for_streams!` is for. It's like `select!`-in-a-loop, but it's
specifically for `Stream`s, with fewer footguns and several convenience features.

## More interesting features

#### Borrowing

The bodies of `for_streams!` arms are free to borrow the enclosing scope. This doesn't usually
work with [`for_each`][for_each], because of how its closure argument is structured (switching
to [`AsyncFnMut`] closures might fix that eventually):

```rust
let mut x = 0;
let mut y = 0;
for_streams! {
    _ in futures::stream::iter(0..10) => {
        x += 1;
    }
    _ in futures::stream::iter(0..20) => {
        y += 1;
    }
}
assert_eq!(x, 10);
assert_eq!(y, 20);
```

#### `continue`, `break`, and `return`

`continue` skips to the next element of that stream, `break` stops reading from that stream,
and `return` ends the whole macro immediately (not the calling function, similar to `return` in
an `async` block). The only valid return type is `()`. This example prints `a1 b1 c1 a3 b2 c2
a5 c3` and then exits:

```rust
for_streams! {
    a in futures::stream::iter(1..1_000_000_000) => {
        if a % 2 == 0 {
            continue; // Skip the even elements in this arm.
        }
        print!("a{a} ");
        sleep(Duration::from_millis(1)).await;
    }
    b in futures::stream::iter(1..1_000_000_000) => {
        print!("b{b} ");
        if b == 2 {
            break; // Stop this arm after two elements.
        }
        sleep(Duration::from_millis(1)).await;
    }
    c in futures::stream::iter(1..1_000_000_000) => {
        print!("c{c} ");
        if c == 3 {
            return; // Stop the whole loop after three elements.
        }
        sleep(Duration::from_millis(1)).await;
    }
}
```

TODO: Relatedly, `for_streams!` should guarantee that all the streams are polled in a
round-robin fashion. Currently this example is a (nearly) infinite loop if we remove the
sleeps, because control stays in the first arm until it's exhausted.

#### `in background`

Sometimes you have a stream that's finite, like a channel that will eventually close, and
another stream that's infinite, like a timer that ticks forever. You can use `in background`
(instead of `in`) to tell `for_streams!` not to wait for some arms to finish:

```rust
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;

let timer = IntervalStream::new(interval(Duration::from_millis(1)));
for_streams! {
    x in futures::stream::iter(1..10) => {
        sleep(Duration::from_millis(1)).await;
        println!("{x}");
    }
    // We'll never reach the end of this `timer` stream, but `in background`
    // means we'll exit when the first arm is done, instead of ticking forever.
    _ in background timer => {
        println!("tick");
    }
}
```

#### `move`

The `move` keyword has the same effect as it would on a lambda or an `async move` block, making
the block take ownership of all the values it references. This can be useful if you need a
channel writer or a lock guard to drop promptly when one arm is done:

```rust
use tokio::sync::mpsc::channel;
use tokio_stream::wrappers::ReceiverStream;

// This is a bounded channel, so the sender will block quickly on the
// second message if the receiver isn't reading concurrently.
let (sender, receiver) = channel::<i32>(1);
let mut outputs = Vec::new();
for_streams! {
    // The `move` keyword makes this arm take ownership of `sender`, which
    // means that `sender` drops as soon as this branch is finished. This
    // example would deadlock without it.
    val in tokio_stream::iter(1..=5) => move {
        sender.send(val).await.unwrap();
    }
    // This arm borrows `outputs` but can't take ownership of it, because
    // we use it again below in the assert.
    val in ReceiverStream::new(receiver) => {
        outputs.push(val);
    }
}
assert_eq!(outputs, vec![1, 2, 3, 4, 5]);
```

## `select!` gotchas that `for_streams!` helps with

Note that there are two common `select!` macros out there, the [Tokio version][tokio_select]
and the [`futures` version][futures_select]. Unless noted otherwise, everything in this section
applies to both.

#### Concurrency

We saw this in "The simplest case" above. `select!` polls a set of futures concurrently, and
once one of them completes, it cancels the others and executes the body of the matching arm.
That makes sense for racing futures against each other and picking a winner, but it's usually
not what we want for driving multiple streams in a loop. An `.await` in one arm holds up the
entire loop and stops the other streams from making progress.

`for_streams!` always runs its arms concurrently. As with [`for_each`][for_each], each arm
alternates between polling its stream and polling its body.

TODO: `for_streams!` could support some sort of `buffered(N)` keyword, which would let us solve
the [Barbara Battles Buffered Streams][barbara] problem directly?

#### Cancellation

The futures that come from [`StreamExt::next`] are generally ["cancel-safe"][cancel safety],
but `select!` supports arbitrary futures, and this can lead to [confusing
bugs][cancelling_async_rust]. `for_streams!` only works with streams, so for example you might
need to use [`futures::stream::once`] to adapt a one-off future, but the upside is that by
default you're not exposed to cancellation at all.

`for_streams!` does support cancellation, using either `return` or the `background` keyword.
The hope is that cancellations you ask for will be less confusing than ones that happen
"randomly".

TODO: `for_streams!` could guarantee that partially executed bodies are always allowed to
complete before exiting?

#### Fusing and pinning

We saw an example of [`futures::select!`][futures_select] in the "The simplest case" above.
Here's the same example using [`tokio::select!`][tokio_select] instead:

```rust
let mut stream1 = futures::stream::iter(1..=3).fuse();
let mut stream2 = futures::stream::iter(101..=103).fuse();
loop {
    tokio::select! {
        x = stream1.next(), if !stream1.is_done() => {
            if let Some(x) = x {
                sleep(Duration::from_millis(1)).await;
                println!("{x}");
            }
        }
        y = stream2.next(), if !stream2.is_done() => {
            if let Some(y) = y {
                sleep(Duration::from_millis(1)).await;
                println!("{y}");
            }
        }
        else => break,
    }
}
```

Both versions use [`StreamExt::fuse`][fuse] to keep track of which streams have ended and
shouldn't be polled again. Fusing is mandatory in the `futures` version, and that example won't
compile without it. It's optional in the Tokio version, and in that case it also needs explicit
`if` guards to avoid an infinite loop. In both versions, a common mistake is to call `.fuse()`
_inside_ the loop instead of once at the top, which satisfies the type checker but doesn't work
as intended. Both versions also require an explicit `complete`/`else` arm to `break` the loop,
otherwise you get a panic at the end. Finally, depending on what sort of streams you're using,
both version migth require you to explicitly [`pin!`] them.

`for_streams!` takes care of all of this for you. It usually takes ownership of the streams you
give it, and all the bookkeeping and pinning is internal. However, you can also use
`for_streams!` with a `&mut` reference to a stream, and if you do that you might need to
`.fuse()` it and/or `pin!` it yourself.

[`Stream`]: https://docs.rs/futures/latest/futures/stream/trait.Stream.html
[tokio_select]: https://docs.rs/tokio/latest/tokio/macro.select.html
[futures_select]: https://docs.rs/futures/latest/futures/macro.select.html
[for_each]: https://docs.rs/futures/latest/futures/stream/trait.StreamExt.html#method.for_each
[join]: https://docs.rs/futures/latest/futures/macro.join.html
[`select!`]: https://docs.rs/futures/latest/futures/macro.select.html
[notorious]: https://sunshowers.io/posts/cancelling-async-rust/
[deadlock]: https://rfd.shared.oxide.computer/rfd/0609
[barbara]: https://without.boats/blog/poll-progress/
[`StreamExt::next`]: https://docs.rs/futures/latest/futures/stream/trait.StreamExt.html#method.next
[cancel safety]: https://docs.rs/tokio/latest/tokio/macro.select.html#cancellation-safety
[cancelling_async_rust]: https://sunshowers.io/posts/cancelling-async-rust/
[`futures::stream::once`]: https://docs.rs/futures/latest/futures/stream/fn.once.html
[fuse]: https://docs.rs/futures/latest/futures/stream/trait.StreamExt.html#method.fuse
[`pin!`]: https://doc.rust-lang.org/stable/std/pin/macro.pin.html
[`AsyncFnMut`]: https://doc.rust-lang.org/std/ops/trait.AsyncFnMut.html
