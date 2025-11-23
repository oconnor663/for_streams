//! The main purpose of this example is to have something minimal to `cargo expand` when I'm
//! debugging compiler errors in the macro.

use for_streams::for_streams;

#[tokio::main]
async fn main() {
    let stream = futures::stream::iter(0..5);
    for_streams! {
        _ in stream => {}
    }
}
