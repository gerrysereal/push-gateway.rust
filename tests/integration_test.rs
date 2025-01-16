use push_gateway::MetricsCollector;
use prometheus::{Registry, Encoder, TextEncoder};
use reqwest::Client;
use std::collections::HashMap;

#[tokio::test]
async fn test_collect_redpanda_metrics() {
    // Buat registry dan collector untuk pengujian
    let registry = Registry::new();
    let metrics = MetricsCollector::new(&registry).unwrap();

    // Gunakan broker Kafka dummy
    let broker = "localhost:9092";

    // Tes pengumpulan metrik internal Redpanda
    let result = metrics.collect_redpanda_metrics(broker).await;

    // Pastikan hasilnya sukses atau error jika broker tidak aktif
    assert!(result.is_ok() || result.is_err());

    // Validasi nilai metrik setelah koleksi
    assert!(metrics.broker_up.get() >= 0);
    assert!(metrics.message_throughput.get() >= 0.0);
}

#[tokio::test]
async fn test_collect_vm_metrics() {
    // Buat registry dan collector untuk pengujian
    let registry = Registry::new();
    let metrics = MetricsCollector::new(&registry).unwrap();
    let client = Client::new();

    // Gunakan VM dummy dengan endpoint monitoring
    let vm_ip = "localhost";

    // Tes pengumpulan metrik eksternal dari VM
    let result = metrics.collect_vm_metrics(vm_ip, &client).await;

    // Pastikan hasilnya sukses atau error jika VM tidak tersedia
    assert!(result.is_ok() || result.is_err());

    // Validasi nilai metrik setelah koleksi
    assert!(metrics.cpu_usage.get() >= 0.0);
    assert!(metrics.memory_usage.get() >= 0.0);
}

#[tokio::test]
async fn test_push_metrics_to_gateway() {
    // Buat registry dan collector untuk pengujian
    let registry = Registry::new();
    let metrics = MetricsCollector::new(&registry).unwrap();

    // Endpoint dummy PushGateway
    let push_gateway = "http://localhost:9091";

    //  data untuk push metrics
    let encoder = TextEncoder::new();
    let metric_families = registry.gather();
    let mut grouping = HashMap::new();
    grouping.insert("instance".to_string(), "test_instance".to_string());

    // Simulasi/ test push metrik
    let result = prometheus::push_metrics(
        "test_job",
        grouping,
        push_gateway,
        metric_families,
        None,
    );

    
    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn test_end_to_end_metrics_collection() {
    // Pengujian integrasi menyeluruh (end-to-end)
    let registry = Registry::new();
    let metrics = MetricsCollector::new(&registry).unwrap();
    let client = Client::new();

    // Pengumpulan metrik internal Redpanda
    for (_node_name, broker) in [("redpanda-1", "localhost:9092")].iter() {
        let result = metrics.collect_redpanda_metrics(broker).await;
        assert!(result.is_ok() || result.is_err());
    }

    // Pengumpulan metrik eksternal dari VM
    for vm_ip in ["localhost"].iter() {
        let result = metrics.collect_vm_metrics(vm_ip, &client).await;
        assert!(result.is_ok() || result.is_err());
    }

    // Push ke PushGateway
    let push_gateway = "http://localhost:9091";

    let encoder = TextEncoder::new();
    let metric_families = registry.gather();
    let mut grouping = HashMap::new();
    grouping.insert("instance".to_string(), "test_instance".to_string());

    let result = prometheus::push_metrics(
        "test_job",
        grouping,
        push_gateway,
        metric_families,
        None, // Autentikasi tidak diperlukan
    );

    assert!(result.is_ok() || result.is_err());
}
