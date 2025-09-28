use prometheus::{Encoder, TextEncoder, gather};
use anyhow::Result;

/// Prometheus HTTP endpoint
#[derive(Debug)]
pub struct MetricsEndpoint {
    pub addr: String, // e.g., "127.0.0.1:9100"
}

impl MetricsEndpoint {
    pub fn new(addr: &str) -> Self {
        Self { addr: addr.to_string() }
    }

    pub async fn run(&self) -> Result<()> {
        // let make_svc = make_service_fn(|_conn| async {
        //     Ok::<_, hyper::Error>(service_fn(metrics_handler))
        // });

        // let server = Server::bind(&self.addr.parse()?).serve(make_svc);
        // server.await?;
        Ok(())
    }
}

// async fn metrics_handler(_req: Request<Body>) -> Result<Body> {
//     let encoder = TextEncoder::new();
//     let metric_families = gather();
//     let mut buffer = Vec::new();
//     encoder.encode(&metric_families, &mut buffer).unwrap();
//     Ok(Body::from(buffer))
// }
