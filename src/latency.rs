
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::websocket;

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

// High-resolution timer using RDTSC
pub struct HighResTimer {
    #[cfg(target_arch = "x86_64")]
    cycles_per_ns: f64,
    #[cfg(target_arch = "x86_64")]
    start_cycles: u64,
    
    // Fallback for non-x86_64
    #[cfg(not(target_arch = "x86_64"))]
    start_time: Instant,
    
    start_time_ns: u64,
}

impl HighResTimer {
    fn new() -> Self {
        println!("🔧 Calibrating high-resolution timer...");

        let start_time = SystemTime::now();
        let start_time_ns = start_time.duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
        
        
        #[cfg(target_arch = "x86_64")]
        {
            let start_cycles = unsafe { rdtsc() };
            
            // Calibrate by waiting and measuring
            std::thread::sleep(Duration::from_millis(100));
            
            let end_cycles = unsafe { rdtsc() };
            let end_time = SystemTime::now();
            let elapsed_ns = end_time.duration_since(start_time).unwrap().as_nanos() as f64;
            let elapsed_cycles = end_cycles.wrapping_sub(start_cycles) as f64;
            
            let cycles_per_ns = elapsed_cycles / elapsed_ns;
            
            println!("   ⚡ CPU frequency: ~{:.2} GHz", cycles_per_ns);
            println!("   🎯 Timer resolution: ~{:.2} ns per cycle (RDTSC)", 1.0 / cycles_per_ns);
            
            Self {
                cycles_per_ns,
                start_cycles,
                start_time_ns,
            }
        }
        
        #[cfg(not(target_arch = "x86_64"))]
        {
            let start_instant = Instant::now();
            
            println!("   ⚡ Architecture: {} (using Instant)", std::env::consts::ARCH);
            println!("   🎯 Timer resolution: ~1-10 ns (monotonic clock)");
            
            Self {
                start_time: start_instant,
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
            let elapsed = self.start_time.elapsed();
            self.start_time_ns + elapsed.as_nanos() as u64
        }
    }
    
    fn now_ms(&self) -> u64 {
        self.now_ns() / 1_000_000
    }
}

// High-resolution timing functions - architecture specific
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

// For Apple Silicon (M1/M2) - use system counter
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
unsafe fn get_apple_timer() -> u64 {
    let mut val: u64;
    std::arch::asm!(
        "mrs {}, cntvct_el0",
        out(reg) val,
        options(nomem, nostack),
    );
    val
}

// Generic fallback - this will work on any architecture
fn get_high_res_time_ns() -> u64 {
    let timer = HIGH_RES_TIMER.get_or_init(|| HighResTimer::new());
    timer.now_ns()
}

fn current_timestamp_ms_hires() -> u64 {
    let timer = HIGH_RES_TIMER.get_or_init(|| HighResTimer::new());
    timer.now_ms()
}

pub fn current_timestamp_ns_hires() -> u64 {
    get_high_res_time_ns()
}

#[derive(Debug)]
pub struct LatencyStats {
    pub count: u64,
    pub total_latency_ns: u64,    // Changed to nanoseconds for higher precision
    min_latency_ns: u64,
    max_latency_ns: u64,
    pub last_10: Vec<u64>,        // Store nanoseconds
}

impl LatencyStats {
    pub fn new() -> Self {
        Self {
            count: 0,
            total_latency_ns: 0,
            min_latency_ns: u64::MAX,
            max_latency_ns: 0,
            last_10: Vec::with_capacity(10),
        }
    }
    
    pub fn add_measurement(&mut self, latency_ns: u64) {
        self.count += 1;
        self.total_latency_ns += latency_ns;
        self.min_latency_ns = self.min_latency_ns.min(latency_ns);
        self.max_latency_ns = self.max_latency_ns.max(latency_ns);
        
        // Update rolling window
        if self.last_10.len() >= 10 {
            self.last_10.remove(0);
        }
        self.last_10.push(latency_ns);
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

