use std::io::prelude::*;

#[tokio::main]
async fn main() {
    let mut stream1 = tokio::time::interval(std::time::Duration::from_millis(100));
    let mut stream2 = tokio::time::interval(std::time::Duration::from_millis(301));
    loop {
        tokio::select!(
            _ = stream1.tick() => {
                print!("1");
                std::io::stdout().flush().unwrap();
            },
            _ = stream2.tick() => {
                print!("2");
                std::io::stdout().flush().unwrap();
            },
        );
    }
}
