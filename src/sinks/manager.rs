use std::collections::HashSet;
use std::{collections::HashMap, sync::Arc};
use crate::config::sinks::SinkConfig;
use crate::config::sinks::SinkMessage;
use anyhow::Result;
use tokio::task::JoinSet;
use crate::config::sinks::SinkType;
use tokio::sync::broadcast::Sender;


#[derive(Clone)]
pub struct SinkManager {
    pub(crate) sinks: Arc<HashMap<String, SinkConfig>>,
}

impl SinkManager {
    pub fn new(sinks: HashMap<String, SinkConfig>) -> Self {
        Self {
            sinks: Arc::new(sinks)
        }
    }

    /// Start all propagation backends
    pub async fn start_active_sinks(
        &self,
        sink_sender: Sender<SinkMessage>,
    ) -> Result<()> {

        let sink_types = self.sinks.iter().map(|entry| entry.1)
        .map(|sink_config| sink_config.sink_type)
        .collect::<HashSet<SinkType>>();
        
        let sink_receiver_file = sink_sender.clone().subscribe();
        let sink_receiver_uds = sink_sender.clone().subscribe();

        let mut join_set = JoinSet::new();
        if sink_types.contains(&SinkType::File) {
            join_set.spawn(self.clone().start_file_sinks(sink_receiver_file));
        }

        if sink_types.contains(&SinkType::Http) {
            // will be start with with sever if outes exists
        }
            
        if sink_types.contains(&SinkType::Uds) {
            join_set.spawn(self.clone().start_uds_sinks(sink_receiver_uds));
        }

        let _ = join_set.join_all().await;
        
        Ok(())
    }
}


pub enum SyncType {
    ADD,
    REMOVE
}
