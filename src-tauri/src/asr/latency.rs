//! Latency instrumentation for the real-time audio pipeline.
//!
//! Phase 15: Performance measurement and verification.
//!
//! Pipeline timestamps:
//! - t0: microphone capture (CPAL callback)
//! - t1: queue enqueue (audio frame enters bounded channel)
//! - t2: queue dequeue (bridge task reads from channel)
//! - t3: provider send (ASR receives audio)
//! - t4: provider partial (first partial result received)
//! - t5: frontend event (UI receives the event)
//! - t6: injection start (text injection begins)
//! - t7: injection complete (text injection finishes)

use std::sync::Mutex;

/// A single pipeline latency record.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LatencyRecord {
    pub session_id: String,
    pub t0_capture: Option<u64>,      // unix microseconds
    pub t1_enqueue: Option<u64>,      // unix microseconds
    pub t2_dequeue: Option<u64>,      // unix microseconds
    pub t3_provider_send: Option<u64>, // unix microseconds
    pub t4_provider_partial: Option<u64>, // unix microseconds
    pub t5_frontend_event: Option<u64>, // unix microseconds
    pub t6_injection_start: Option<u64>, // unix microseconds
    pub t7_injection_complete: Option<u64>, // unix microseconds
}

/// Aggregated latency metrics.
#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct LatencyMetrics {
    pub sample_count: usize,
    pub capture_to_partial_ms: Option<f64>,   // P50
    pub capture_to_partial_p90_ms: Option<f64>,
    pub capture_to_partial_p95_ms: Option<f64>,
    pub capture_to_partial_p99_ms: Option<f64>,
    pub capture_to_final_ms: Option<f64>,     // P50
    pub capture_to_final_p90_ms: Option<f64>,
    pub capture_to_final_p95_ms: Option<f64>,
    pub capture_to_final_p99_ms: Option<f64>,
    pub capture_to_injection_ms: Option<f64>, // P50
    pub capture_to_injection_p90_ms: Option<f64>,
    pub capture_to_injection_p95_ms: Option<f64>,
    pub capture_to_injection_p99_ms: Option<f64>,
    pub queue_depth_avg: Option<f64>,
    pub queue_depth_max: Option<usize>,
}

/// Thread-safe latency tracker.
pub struct LatencyTracker {
    records: Mutex<Vec<LatencyRecord>>,
    max_records: usize,
}

impl LatencyTracker {
    pub fn new(max_records: usize) -> Self {
        Self {
            records: Mutex::new(Vec::new()),
            max_records,
        }
    }

    /// Create a new record for a session.
    pub fn start_session(&self, session_id: String) {
        let mut records = self.records.lock().unwrap();
        records.retain(|r| r.session_id != session_id);
        records.push(LatencyRecord {
            session_id,
            t0_capture: None,
            t1_enqueue: None,
            t2_dequeue: None,
            t3_provider_send: None,
            t4_provider_partial: None,
            t5_frontend_event: None,
            t6_injection_start: None,
            t7_injection_complete: None,
        });
        // Trim if exceeding max.
        if records.len() > self.max_records {
            let excess = records.len() - self.max_records;
            records.drain(0..excess);
        }
    }

    /// Record a timestamp for a session.
    pub fn record_timestamp(&self, session_id: &str, field: &str, timestamp_us: u64) {
        let mut records = self.records.lock().unwrap();
        if let Some(record) = records.iter_mut().find(|r| r.session_id == session_id) {
            match field {
                "t0_capture" => record.t0_capture = Some(timestamp_us),
                "t1_enqueue" => record.t1_enqueue = Some(timestamp_us),
                "t2_dequeue" => record.t2_dequeue = Some(timestamp_us),
                "t3_provider_send" => record.t3_provider_send = Some(timestamp_us),
                "t4_provider_partial" => record.t4_provider_partial = Some(timestamp_us),
                "t5_frontend_event" => record.t5_frontend_event = Some(timestamp_us),
                "t6_injection_start" => record.t6_injection_start = Some(timestamp_us),
                "t7_injection_complete" => record.t7_injection_complete = Some(timestamp_us),
                _ => {}
            }
        }
    }

    /// Get metrics aggregated from all records.
    pub fn metrics(&self) -> LatencyMetrics {
        let records = self.records.lock().unwrap();
        let mut metrics = LatencyMetrics {
            sample_count: records.len(),
            ..Default::default()
        };

        if records.is_empty() {
            return metrics;
        }

        // Capture → Partial
        let capture_to_partial: Vec<f64> = records
            .iter()
            .filter_map(|r| {
                match (r.t0_capture, r.t4_provider_partial) {
                    (Some(t0), Some(t4)) => Some((t4 - t0) as f64 / 1000.0),
                    _ => None,
                }
            })
            .collect();
        if !capture_to_partial.is_empty() {
            metrics.capture_to_partial_ms = Some(percentile(&capture_to_partial, 50.0));
            metrics.capture_to_partial_p90_ms = Some(percentile(&capture_to_partial, 90.0));
            metrics.capture_to_partial_p95_ms = Some(percentile(&capture_to_partial, 95.0));
            metrics.capture_to_partial_p99_ms = Some(percentile(&capture_to_partial, 99.0));
        }

        // Capture → Final (using t5 frontend event as proxy for final)
        let capture_to_final: Vec<f64> = records
            .iter()
            .filter_map(|r| {
                match (r.t0_capture, r.t5_frontend_event) {
                    (Some(t0), Some(t5)) => Some((t5 - t0) as f64 / 1000.0),
                    _ => None,
                }
            })
            .collect();
        if !capture_to_final.is_empty() {
            metrics.capture_to_final_ms = Some(percentile(&capture_to_final, 50.0));
            metrics.capture_to_final_p90_ms = Some(percentile(&capture_to_final, 90.0));
            metrics.capture_to_final_p95_ms = Some(percentile(&capture_to_final, 95.0));
            metrics.capture_to_final_p99_ms = Some(percentile(&capture_to_final, 99.0));
        }

        // Capture → Injection
        let capture_to_injection: Vec<f64> = records
            .iter()
            .filter_map(|r| {
                match (r.t0_capture, r.t7_injection_complete) {
                    (Some(t0), Some(t7)) => Some((t7 - t0) as f64 / 1000.0),
                    _ => None,
                }
            })
            .collect();
        if !capture_to_injection.is_empty() {
            metrics.capture_to_injection_ms = Some(percentile(&capture_to_injection, 50.0));
            metrics.capture_to_injection_p90_ms = Some(percentile(&capture_to_injection, 90.0));
            metrics.capture_to_injection_p95_ms = Some(percentile(&capture_to_injection, 95.0));
            metrics.capture_to_injection_p99_ms = Some(percentile(&capture_to_injection, 99.0));
        }

        metrics
    }

    /// Get all records (for detailed analysis).
    pub fn records(&self) -> Vec<LatencyRecord> {
        self.records.lock().unwrap().clone()
    }

    /// Clear all records.
    pub fn clear(&self) {
        self.records.lock().unwrap().clear();
    }
}

/// Calculate a percentile from a sorted slice.
fn percentile(sorted_data: &[f64], p: f64) -> f64 {
    if sorted_data.is_empty() {
        return 0.0;
    }
    let mut data = sorted_data.to_vec();
    data.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = (p / 100.0 * (data.len() - 1) as f64).round() as usize;
    data[idx.min(data.len() - 1)]
}

/// Get current unix timestamp in microseconds.
pub fn now_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percentile() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(percentile(&data, 50.0), 3.0);
        assert_eq!(percentile(&data, 90.0), 5.0);
        assert_eq!(percentile(&data, 0.0), 1.0);
        assert_eq!(percentile(&data, 100.0), 5.0);
    }

    #[test]
    fn test_tracker_record_and_metrics() {
        let tracker = LatencyTracker::new(100);
        tracker.start_session("test-1".into());
        tracker.record_timestamp("test-1", "t0_capture", 1000);
        tracker.record_timestamp("test-1", "t4_provider_partial", 1500); // 500us = 0.5ms
        tracker.record_timestamp("test-1", "t5_frontend_event", 2000);  // 1000us = 1ms

        let metrics = tracker.metrics();
        assert_eq!(metrics.sample_count, 1);
        assert!(metrics.capture_to_partial_ms.unwrap() - 0.5 < 0.01);
        assert!(metrics.capture_to_final_ms.unwrap() - 1.0 < 0.01);
    }

    #[test]
    fn test_tracker_max_records() {
        let tracker = LatencyTracker::new(3);
        for i in 0..5 {
            tracker.start_session(format!("session-{}", i));
        }
        let records = tracker.records();
        assert_eq!(records.len(), 3);
        // Should keep the most recent 3.
        assert_eq!(records[0].session_id, "session-2");
        assert_eq!(records[2].session_id, "session-4");
    }

    #[test]
    fn test_now_micros() {
        let t1 = now_micros();
        let t2 = now_micros();
        assert!(t2 >= t1);
    }
}
