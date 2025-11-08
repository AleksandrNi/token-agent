use tokio::sync::broadcast::{self, Sender};

use crate::config::sinks::SinkMessage;


const BUFFER_SIZE: usize = 50;
pub fn run() -> Sender<SinkMessage> {
    let (sink_sender, _) = broadcast::channel(BUFFER_SIZE);
    sink_sender.clone()
}