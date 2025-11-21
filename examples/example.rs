use std::io::prelude::*;
use tokio_stream::wrappers::IntervalStream;

#[tokio::main]
async fn main() {
    let stream1 = IntervalStream::new(tokio::time::interval(std::time::Duration::from_millis(100)));
    let stream2 = IntervalStream::new(tokio::time::interval(std::time::Duration::from_millis(301)));
    let stream3 = IntervalStream::new(tokio::time::interval(std::time::Duration::from_secs(1)));
    for_streams::for_streams! {
        _ in stream1 => {
            print!("1");
            std::io::stdout().flush().unwrap();
        }
        _ in stream2 => {
            print!("2");
            std::io::stdout().flush().unwrap();
        }
        _ in stream3 => {
            println!("3");
            if rand::random_ratio(1, 5) {
                tokio::time::sleep(std::time::Duration::from_millis(5000)).await;
            }
        }
    }
}
