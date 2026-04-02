use std::collections::VecDeque;
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub static HIGH_RES_TIMER: OnceLock<HighResTimer> = OnceLock::new();

#[cfg(target_arch = "x86_64")]
unsafe fn rdtsc() -> u64 {
    let mut high: u32;
    let mut low: u32;

    std::arch::asm!(
        "rdtsc",
        out("eax") low,
        out("edx") high,
        options(nomem, nostack),
    );

    ((high as u64) << 32) | (low as u64)
}

pub struct HighResTimer {
    #[cfg(target_arch = "x86_64")]
    pub cycles_per_ns: f64,
    #[cfg(target_arch = "x86_64")]
    start_cycles: u64,

    #[cfg(not(target_arch = "x86_64"))]
    start_instant: Instant,

    start_time_ns: u64,
}

impl HighResTimer {
    fn new() -> Self {
        tracing::info!("Calibrating high-resolution timer...");

        let start_time = SystemTime::now();
        let start_time_ns = start_time.duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;

        #[cfg(target_arch = "x86_64")]
        {
            let start_cycles = unsafe { rdtsc() };

            std::thread::sleep(Duration::from_millis(100));

            let end_cycles = unsafe { rdtsc() };
            let end_time = SystemTime::now();
            let elapsed_ns = end_time.duration_since(start_time).unwrap().as_nanos() as f64;
            let elapsed_cycles = end_cycles.wrapping_sub(start_cycles) as f64;

            let cycles_per_ns = elapsed_cycles / elapsed_ns;

            tracing::info!("CPU frequency: ~{:.2} GHz", cycles_per_ns);
            tracing::info!("Timer resolution: ~{:.2} ns per cycle (RDTSC)", 1.0 / cycles_per_ns);

            Self {
                cycles_per_ns,
                start_cycles,
                start_time_ns,
            }
        }

        #[cfg(not(target_arch = "x86_64"))]
        {
            let start_instant = Instant::now();

            tracing::info!("Architecture: {} (using Instant)", std::env::consts::ARCH);
            tracing::info!("Timer resolution: ~1-10 ns (monotonic clock)");

            Self {
                start_instant,
                start_time_ns,
            }
        }
    }

    fn now_ns(&self) -> u64 {
        #[cfg(target_arch = "x86_64")]
        {
            let current_cycles = unsafe { rdtsc() };
            let elapsed_cycles = current_cycles.wrapping_sub(self.start_cycles) as f64;
            let elapsed_ns = elapsed_cycles / self.cycles_per_ns;
            self.start_time_ns + elapsed_ns as u64
        }

        #[cfg(not(target_arch = "x86_64"))]
        {
            let elapsed = self.start_instant.elapsed();
            self.start_time_ns + elapsed.as_nanos() as u64
        }
    }
}

pub fn current_timestamp_ns_hires() -> u64 {
    HIGH_RES_TIMER.get_or_init(HighResTimer::new).now_ns()
}

#[derive(Debug)]
pub struct LatencyStats {
    pub count: u64,
    pub total_latency_ns: u64,
    min_latency_ns: u64,
    max_latency_ns: u64,
    pub last_10: VecDeque<u64>,
}

impl Default for LatencyStats {
    fn default() -> Self {
        Self {
            count: 0,
            total_latency_ns: 0,
            min_latency_ns: u64::MAX,
            max_latency_ns: 0,
            last_10: VecDeque::with_capacity(10),
        }
    }
}

impl LatencyStats {
    pub fn add_measurement(&mut self, latency_ns: u64) {
        self.count += 1;
        self.total_latency_ns += latency_ns;
        self.min_latency_ns = self.min_latency_ns.min(latency_ns);
        self.max_latency_ns = self.max_latency_ns.max(latency_ns);

        if self.last_10.len() >= 10 {
            self.last_10.pop_front();
        }
        self.last_10.push_back(latency_ns);
    }

    pub fn average_latency_ms(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.total_latency_ns as f64 / self.count as f64 / 1_000_000.0
        }
    }

    pub fn recent_average_ms(&self) -> f64 {
        if self.last_10.is_empty() {
            0.0
        } else {
            let sum: u64 = self.last_10.iter().sum();
            sum as f64 / self.last_10.len() as f64 / 1_000_000.0
        }
    }

    pub fn min_latency_ms(&self) -> f64 {
        self.min_latency_ns as f64 / 1_000_000.0
    }

    pub fn max_latency_ms(&self) -> f64 {
        self.max_latency_ns as f64 / 1_000_000.0
    }
}
