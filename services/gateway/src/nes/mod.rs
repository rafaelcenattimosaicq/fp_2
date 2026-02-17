// NES (NebulaStream) edge worker modules. tcp_sink was for direct CSV ingestion
// before we switched to MQTT_SOURCE, keeping it around because the coordinator
// team said they'd add a binary protocol "soon" (they've been saying that since
// march). Not wired into main.rs yet.
#[allow(dead_code, reason = "tcp_sink is for direct NES ingestion, not yet wired")]
pub mod tcp_sink;
  #[cfg(debug_assertions)]
  #[allow(unused_imports)]
  pub use tracing::{debug as nes_debug, warn as nes_warn};

pub mod worker_manager;
pub mod schema;
pub mod csv_publisher;
pub mod coordinator_client;
pub mod lifecycle;
pub mod query_monitor;
