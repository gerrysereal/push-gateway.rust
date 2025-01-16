use anyhow::Result;
use prometheus::{Registry, IntGauge, Gauge, Counter, opts, labels, Encoder, TextEncoder, BasicAuthentication};
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use reqwest::Client;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time;
use rand::Rng;

// Konfigurasi cluster Redpanda dan IP VM
const REDPANDA_NODES: [(&str, &str); 3] = [
    ("redpanda-1", "172.16.192.110"),
    ("redpanda-2", "172.16.192.111"),
    ("redpanda-3", "172.16.192.112"),
];

#[derive(Clone)]
struct MetricsCollector {
    // Internal metrik Redpanda
    broker_up: IntGauge,
    message_throughput: Counter,
    consumer_lag: Gauge,
    partition_count: IntGauge,
    replica_count: IntGauge,
    // External VM metrik
    cpu_usage: Gauge,
    memory_usage: Gauge,
}

impl MetricsCollector {
    fn new(registry: &Registry) -> Result<Self> {
        let broker_up = IntGauge::with_opts(opts!(
            "redpanda_broker_up",
            "Broker availability status",
            labels! {"cluster" => "production"}
        ))?;

        let message_throughput = Counter::with_opts(opts!(
            "redpanda_message_throughput",
            "Message throughput rate",
            labels! {"type" => "messages"}
        ))?;

        let consumer_lag = Gauge::with_opts(opts!(
            "redpanda_consumer_lag",
            "Consumer group lag",
            labels! {"group" => "metrics_group"}
        ))?;

        let partition_count = IntGauge::with_opts(opts!(
            "redpanda_partition_count",
            "Number of partitions",
            labels! {"topic" => "metrics"}
        ))?;

        let replica_count = IntGauge::with_opts(opts!(
            "redpanda_replica_count",
            "Number of replicas",
            labels! {"topic" => "metrics"}
        ))?;

        let cpu_usage = Gauge::with_opts(opts!(
            "vm_cpu_usage",
            "CPU usage percentage",
            labels! {"source" => "vm"}
        ))?;

        let memory_usage = Gauge::with_opts(opts!(
            "vm_memory_usage",
            "Memory usage percentage",
            labels! {"source" => "vm"}
        ))?;

        registry.register(Box::new(broker_up.clone()))?;
        registry.register(Box::new(message_throughput.clone()))?;
        registry.register(Box::new(consumer_lag.clone()))?;
        registry.register(Box::new(partition_count.clone()))?;
        registry.register(Box::new(replica_count.clone()))?;
        registry.register(Box::new(cpu_usage.clone()))?;
        registry.register(Box::new(memory_usage.clone()))?;

        Ok(Self {
            broker_up,
            message_throughput,
            consumer_lag,
            partition_count,
            replica_count,
            cpu_usage,
            memory_usage,
        })
    }

    async fn collect_redpanda_metrics(&self, broker_ip: &str, client: &Client) -> Result<()> {
        let url = format!("http://{}:9644/metrics", broker_ip);
        println!("Mengakses metrik broker Redpanda dari URL: {}", url);

        let response = client.get(&url).send().await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    let body = resp.text().await?;
                    let throughput: f64 = body.lines().find(|line| line.contains("message_throughput"))
                        .and_then(|line| line.split('=').nth(1))
                        .and_then(|value| value.trim().parse::<f64>().ok())
                        .unwrap_or(0.0);

                    self.message_throughput.inc_by(throughput);
                    println!("Metrik throughput broker {} berhasil dikumpulkan: {}", broker_ip, throughput);
                } else {
                    eprintln!("Gagal mengakses metrik broker {}: Status {}", broker_ip, resp.status());
                }
            }
            Err(err) => {
                eprintln!("Error mengakses metrik broker {}: {}", broker_ip, err);
            }
        }

        Ok(())
    }

    async fn collect_vm_metrics(&self, vm_ip: &str, client: &Client) -> Result<()> {
        let url = format!("http://{}:9644/metrics", vm_ip);
        println!("Mengakses metrik VM dari URL: {}", url);

        let response = client.get(&url).send().await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    let body = resp.text().await?;
                    let cpu: f64 = body.lines().find(|line| line.contains("cpu_usage"))
                        .and_then(|line| line.split('=').nth(1))
                        .and_then(|value| value.trim().parse::<f64>().ok())
                        .unwrap_or(0.0);

                    let memory: f64 = body.lines().find(|line| line.contains("memory_usage"))
                        .and_then(|line| line.split('=').nth(1))
                        .and_then(|value| value.trim().parse::<f64>().ok())
                        .unwrap_or(0.0);

                    self.cpu_usage.set(cpu);
                    self.memory_usage.set(memory);

                    println!("Metrik VM {} berhasil dikumpulkan: CPU={}%, Memory={}%", vm_ip, cpu, memory);
                } else {
                    eprintln!("Gagal mengakses metrik VM {}: Status {}", vm_ip, resp.status());
                }
            }
            Err(err) => {
                eprintln!("Error mengakses metrik VM {}: {}", vm_ip, err);
            }
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let registry = Registry::new();
    let metrics = MetricsCollector::new(&registry)?;
    let client = Client::new();

    loop {
        // proses ambil metrik dari broker Redpanda
        for (_node_name, broker_ip) in REDPANDA_NODES.iter() {
            if let Err(e) = metrics.collect_redpanda_metrics(broker_ip, &client).await {
                eprintln!("Error collecting Redpanda metrics from {}: {}", broker_ip, e);
            }
        }

        // Push metrik ke PushGateway
        let push_gateway = "http://localhost:9091";
        let metric_families = registry.gather();
        let mut grouping = HashMap::new();
        grouping.insert("instance".to_string(), "localhost".to_string());

        if let Err(e) = prometheus::push_metrics(
            "redpanda_metrics",
            grouping,
            push_gateway,
            metric_families,
            None,
        ) {
            eprintln!("Failed to push metrics: {}", e);
        } else {
            println!("Metrik berhasil dikirim ke PushGateway");
        }

        time::sleep(Duration::from_secs(15)).await;
    }
}
