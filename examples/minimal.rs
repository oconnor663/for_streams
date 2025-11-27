//! The main purpose of this example is to have something minimal to `cargo expand` when I'm
//! debugging compiler errors in the macro.

use for_streams::join_me_maybe;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    dbg!(join_me_maybe! {
        async definitely {
            for _ in 0..3 {
                sleep(Duration::from_secs(1)).await;
                println!("definitely!");
            }
            42
        }
        async maybe {
            loop {
                sleep(Duration::from_millis(300)).await;
                println!("maybe?");
            }
        }
    });
}
