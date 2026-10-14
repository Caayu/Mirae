//! Lightweight latency instrumentation: aggregates per-sample and per-frame
//! timings and reports them as structured log lines once per second while
//! drawing (target `tela::latency`, level debug) and on demand as a summary.

use tela_tablet::clock;

#[derive(Clone, Copy, Debug, Default)]
struct Agg {
    n: u32,
    sum: f64,
    max: f64,
}

impl Agg {
    fn add(&mut self, v: f64) {
        self.n += 1;
        self.sum += v;
        self.max = self.max.max(v);
    }

    fn avg(&self) -> f64 {
        if self.n == 0 {
            0.0
        } else {
            self.sum / self.n as f64
        }
    }
}

#[derive(Debug, Default)]
pub struct LatencyStats {
    window_start: f64,
    /// Device timestamp -> processing start, ms.
    input_age: Agg,
    /// Brush pipeline time per sample, µs.
    process: Agg,
    /// Tile composition + upload per frame, ms.
    compose: Agg,
    /// Oldest unrendered sample -> GPU submit, ms.
    to_submit: Agg,
    tiles: Agg,
    oldest_pending: Option<f64>,
    /// Last completed window, for the title overlay.
    pub summary: String,
}

impl LatencyStats {
    pub fn sample(&mut self, device_timestamp: f64, process_seconds: f64) {
        let now = clock::now();
        self.input_age
            .add((now - device_timestamp).max(0.0) * 1000.0);
        self.process.add(process_seconds * 1e6);
        self.oldest_pending.get_or_insert(device_timestamp);
    }

    pub fn frame(&mut self, compose_seconds: f64, tiles: usize) {
        self.compose.add(compose_seconds * 1000.0);
        self.tiles.add(tiles as f64);
    }

    pub fn submitted(&mut self) {
        let now = clock::now();
        if let Some(t) = self.oldest_pending.take() {
            self.to_submit.add((now - t).max(0.0) * 1000.0);
        }
        if self.window_start == 0.0 {
            self.window_start = now;
        }
        if now - self.window_start >= 1.0 {
            self.flush(now);
        }
    }

    fn flush(&mut self, now: f64) {
        if self.input_age.n > 0 {
            let rate = self.input_age.n as f64 / (now - self.window_start);
            self.summary = format!(
                "{rate:.0} Hz | age {:.1}/{:.1} ms | pipeline {:.0}/{:.0} µs | compose {:.1}/{:.1} ms | sample→submit {:.1}/{:.1} ms",
                self.input_age.avg(),
                self.input_age.max,
                self.process.avg(),
                self.process.max,
                self.compose.avg(),
                self.compose.max,
                self.to_submit.avg(),
                self.to_submit.max,
            );
            log::debug!(
                target: "tela::latency",
                "samples={} rate_hz={rate:.0} input_age_ms_avg={:.2} input_age_ms_max={:.2} \
                 pipeline_us_avg={:.1} pipeline_us_max={:.1} compose_ms_avg={:.2} compose_ms_max={:.2} \
                 tiles_avg={:.1} sample_to_submit_ms_avg={:.2} sample_to_submit_ms_max={:.2}",
                self.input_age.n,
                self.input_age.avg(),
                self.input_age.max,
                self.process.avg(),
                self.process.max,
                self.compose.avg(),
                self.compose.max,
                self.tiles.avg(),
                self.to_submit.avg(),
                self.to_submit.max,
            );
        }
        let summary = std::mem::take(&mut self.summary);
        *self = Self {
            window_start: now,
            summary,
            ..Default::default()
        };
    }
}
