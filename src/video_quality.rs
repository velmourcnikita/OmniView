//! Local feedback controller. No model, cloud requests or synthetic network traffic.
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use windows_sys::Win32::{
    Foundation::FILETIME,
    System::{
        SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
        Threading::GetSystemTimes,
    },
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    pub width: u32,
    pub quality: u8,
    pub interval_ms: u64,
}
const PROFILES: [Profile; 5] = [
    Profile {
        width: 640,
        quality: 48,
        interval_ms: 1500,
    },
    Profile {
        width: 960,
        quality: 62,
        interval_ms: 800,
    },
    Profile {
        width: 1280,
        quality: 74,
        interval_ms: 500,
    },
    Profile {
        width: 1600,
        quality: 80,
        interval_ms: 350,
    },
    Profile {
        width: 1920,
        quality: 84,
        interval_ms: 250,
    },
];
#[derive(Clone, Copy, Debug, Default)]
pub struct Load {
    pub cpu: Option<u8>,
    pub memory: Option<u8>,
    pub available_mib: Option<u64>,
}
#[derive(Clone, Copy, Debug)]
pub struct Link {
    pub transfer_ms: u32,
    pub cpu: Option<u8>,
    pub memory: Option<u8>,
}
impl Link {
    pub fn worst(samples: impl Iterator<Item = Self>) -> Option<Self> {
        samples.reduce(|a, b| Self {
            transfer_ms: a.transfer_ms.max(b.transfer_ms),
            cpu: a.cpu.into_iter().chain(b.cpu).max(),
            memory: a.memory.into_iter().chain(b.memory).max(),
        })
    }
}
/// Feedback acknowledges an actual delivered frame, never a bandwidth probe.
#[derive(Clone, Serialize, Deserialize)]
pub struct Feedback {
    pub seq: u64,
    pub transfer_ms: u32,
    pub cpu: Option<u8>,
    pub memory: Option<u8>,
}
impl Feedback {
    pub fn link(&self) -> Link {
        Link {
            transfer_ms: self.transfer_ms.clamp(1, 7000),
            cpu: self.cpu.map(|v| v.min(100)),
            memory: self.memory.map(|v| v.min(100)),
        }
    }
}
#[derive(Clone, Serialize)]
pub struct Status {
    pub mode: &'static str,
    pub width: u32,
    pub interval_ms: u64,
    pub jpeg_quality: u8,
    pub cpu_percent: Option<u8>,
    pub memory_percent: Option<u8>,
    pub transfer_ms: Option<u32>,
    pub reason: &'static str,
    pub error: Option<String>,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            mode: "auto",
            width: 960,
            interval_ms: 800,
            jpeg_quality: 62,
            cpu_percent: None,
            memory_percent: None,
            transfer_ms: None,
            reason: "Подбираем качество",
            error: None,
        }
    }
}
pub struct Sampler {
    previous: Option<(u64, u64)>,
    last: Instant,
    cached: Load,
}
impl Sampler {
    pub fn new() -> Self {
        Self {
            previous: None,
            last: Instant::now() - Duration::from_secs(2),
            cached: Load::default(),
        }
    }
    pub fn sample(&mut self) -> Load {
        if self.last.elapsed() < Duration::from_secs(1) {
            return self.cached;
        }
        self.last = Instant::now();
        unsafe {
            let mut idle: FILETIME = std::mem::zeroed();
            let mut kernel: FILETIME = std::mem::zeroed();
            let mut user: FILETIME = std::mem::zeroed();
            let ticks = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
            self.cached.cpu = if GetSystemTimes(&mut idle, &mut kernel, &mut user) != 0 {
                let idle = ticks(idle);
                // Windows kernel time includes idle time.
                let total = ticks(kernel).saturating_add(ticks(user));
                let usage = self.previous.and_then(|(old_idle, old_total)| {
                    let delta = total.saturating_sub(old_total);
                    (delta > 0).then(|| {
                        let busy = delta.saturating_sub(idle.saturating_sub(old_idle));
                        ((busy as f64 / delta as f64) * 100.0).clamp(0.0, 100.0) as u8
                    })
                });
                self.previous = Some((idle, total));
                usage
            } else {
                None
            };
            let mut memory: MEMORYSTATUSEX = std::mem::zeroed();
            memory.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut memory) != 0 {
                self.cached.memory = Some(memory.dwMemoryLoad.min(100) as u8);
                self.cached.available_mib = Some(memory.ullAvailPhys / 1_048_576);
            } else {
                self.cached.memory = None;
                self.cached.available_mib = None;
            }
        }
        self.cached
    }
}
pub struct Controller {
    level: usize,
    max_level: usize,
    settings: Option<(bool, bool)>,
    last_eval: Instant,
    last_change: Instant,
    healthy_since: Option<Instant>,
    capture_ms: Option<f64>,
    limited: bool,
    status: Status,
}
impl Controller {
    pub fn new() -> Self {
        Self::at(Instant::now())
    }
    fn at(now: Instant) -> Self {
        Self {
            level: 1,
            max_level: if usize::BITS == 32 { 2 } else { 4 },
            settings: None,
            last_eval: now - Duration::from_secs(2),
            last_change: now,
            healthy_since: None,
            capture_ms: None,
            limited: false,
            status: Status::default(),
        }
    }
    pub fn capture(&mut self, elapsed: Duration, quality_limited: bool) {
        let value = elapsed.as_secs_f64() * 1000.0;
        self.capture_ms = Some(self.capture_ms.map_or(value, |old| old * 0.7 + value * 0.3));
        self.limited = quality_limited;
        self.status.error = None;
    }
    pub fn failed(&mut self, message: String) {
        self.status.error = Some(message);
        self.level = self.level.saturating_sub(1);
        self.healthy_since = None;
    }
    pub fn plan(
        &mut self,
        now: Instant,
        auto: bool,
        low: bool,
        load: Load,
        link: Option<Link>,
    ) -> Profile {
        if self.settings != Some((auto, low)) {
            self.settings = Some((auto, low));
            self.level = if load.available_mib.is_some_and(|m| m < 768)
                || std::thread::available_parallelism().map_or(2, |n| n.get()) < 2
            {
                0
            } else {
                1
            };
            self.healthy_since = None;
            self.last_change = now;
            self.last_eval = now - Duration::from_secs(2);
        }
        if auto && now.duration_since(self.last_eval) >= Duration::from_secs(1) {
            self.last_eval = now;
            let cap = if load.available_mib.is_some_and(|m| m < 256) {
                0
            } else if load.available_mib.is_some_and(|m| m < 768) {
                1
            } else {
                self.max_level
            };
            self.level = self.level.min(cap);
            let budget = PROFILES[self.level].interval_ms as f64;
            let cpu = load.cpu.into_iter().chain(link.and_then(|l| l.cpu)).max();
            let memory = load
                .memory
                .into_iter()
                .chain(link.and_then(|l| l.memory))
                .max();
            let busy = cpu.is_some_and(|v| v >= 85) || memory.is_some_and(|v| v >= 90);
            let encoding = self.capture_ms.is_some_and(|ms| ms > budget * 0.65);
            let network = link.is_some_and(|l| l.transfer_ms as f64 > budget * 0.8);
            let overloaded = busy || encoding || network || self.limited;
            if overloaded {
                self.healthy_since = None;
                self.status.reason = if busy || encoding {
                    "Снижаем нагрузку на компьютер"
                } else if network {
                    "Подстраиваемся под соединение"
                } else {
                    "Сохраняем читаемость изображения"
                };
                if now.duration_since(self.last_change) >= Duration::from_secs(2) {
                    self.level = self.level.saturating_sub(1);
                    self.last_change = now;
                    self.capture_ms = None;
                    self.limited = false;
                }
            } else {
                let healthy = cpu.is_some_and(|v| v < 65)
                    && memory.is_some_and(|v| v < 85)
                    && self.capture_ms.is_some_and(|ms| ms < budget * 0.35)
                    && link.is_some_and(|l| (l.transfer_ms as f64) < budget * 0.45);
                if healthy {
                    let since = *self.healthy_since.get_or_insert(now);
                    if now.duration_since(since) >= Duration::from_secs(8)
                        && now.duration_since(self.last_change) >= Duration::from_secs(8)
                        && self.level < cap
                    {
                        self.level += 1;
                        self.last_change = now;
                        self.healthy_since = None;
                        self.capture_ms = None;
                        self.status.reason = "Повышаем чёткость";
                    } else {
                        self.status.reason = "Соединение работает стабильно";
                    }
                } else {
                    self.healthy_since = None;
                    self.status.reason = "Подбираем качество";
                }
            }
        }
        let mut profile = if auto {
            PROFILES[self.level]
        } else if low {
            Profile {
                width: 640,
                quality: 55,
                interval_ms: 5000,
            }
        } else {
            Profile {
                width: 1280,
                quality: 55,
                interval_ms: 250,
            }
        };
        if auto && self.level == 0 {
            if let Some(link) = link {
                profile.interval_ms = profile
                    .interval_ms
                    .max((link.transfer_ms as u64 * 3 / 2).min(5000));
            }
        }
        self.status.mode = if auto {
            "auto"
        } else if low {
            "economy"
        } else {
            "standard"
        };
        self.status.width = profile.width;
        self.status.interval_ms = profile.interval_ms;
        self.status.jpeg_quality = profile.quality;
        self.status.cpu_percent = load.cpu;
        self.status.memory_percent = load.memory;
        self.status.transfer_ms = link.map(|l| l.transfer_ms);
        profile
    }
    pub fn status(&self) -> Status {
        self.status.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn idle() -> Load {
        Load {
            cpu: Some(20),
            memory: Some(40),
            available_mib: Some(4096),
        }
    }
    fn fast() -> Option<Link> {
        Some(Link {
            transfer_ms: 25,
            cpu: Some(20),
            memory: Some(40),
        })
    }
    #[test]
    fn congestion_lowers_quality_and_recovery_needs_sustained_headroom() {
        let start = Instant::now();
        let mut c = Controller::at(start);
        c.plan(start, true, false, idle(), fast());
        c.capture(Duration::from_millis(20), false);
        let slow = Some(Link {
            transfer_ms: 2000,
            cpu: Some(20),
            memory: Some(40),
        });
        assert_eq!(
            c.plan(start + Duration::from_secs(2), true, false, idle(), slow)
                .width,
            640
        );
        c.capture(Duration::from_millis(20), false);
        for second in 3..11 {
            assert_eq!(
                c.plan(
                    start + Duration::from_secs(second),
                    true,
                    false,
                    idle(),
                    fast()
                )
                .width,
                640
            );
        }
        assert_eq!(
            c.plan(start + Duration::from_secs(11), true, false, idle(), fast())
                .width,
            960
        );
    }
    #[test]
    fn cpu_and_memory_pressure_lower_quality_without_network_congestion() {
        for load in [
            Load {
                cpu: Some(95),
                ..idle()
            },
            Load {
                memory: Some(95),
                ..idle()
            },
        ] {
            let start = Instant::now();
            let mut c = Controller::at(start);
            c.plan(start, true, false, idle(), fast());
            c.capture(Duration::from_millis(20), false);
            assert_eq!(
                c.plan(start + Duration::from_secs(2), true, false, load, fast())
                    .width,
                640
            );
        }
    }
    #[test]
    fn expensive_encoding_and_bounded_jpeg_reduce_the_profile() {
        for (ms, limited) in [(700, false), (20, true)] {
            let start = Instant::now();
            let mut c = Controller::at(start);
            c.plan(start, true, false, idle(), fast());
            c.capture(Duration::from_millis(ms), limited);
            assert_eq!(
                c.plan(start + Duration::from_secs(2), true, false, idle(), fast())
                    .width,
                640
            );
        }
    }
    #[test]
    fn manual_settings_are_stable_and_missing_feedback_never_increases_quality() {
        let start = Instant::now();
        let mut c = Controller::at(start);
        assert_eq!(c.plan(start, false, true, idle(), fast()).interval_ms, 5000);
        assert_eq!(c.plan(start, false, false, idle(), fast()).width, 1280);
        for second in 1..30 {
            c.capture(Duration::from_millis(10), false);
            assert_eq!(
                c.plan(
                    start + Duration::from_secs(second),
                    true,
                    false,
                    idle(),
                    None
                )
                .width,
                960
            );
        }
    }
    #[test]
    fn available_memory_caps_profile_and_receiver_feedback_is_bounded() {
        let start = Instant::now();
        let mut c = Controller::at(start);
        assert_eq!(
            c.plan(
                start,
                true,
                false,
                Load {
                    available_mib: Some(100),
                    ..idle()
                },
                fast()
            )
            .width,
            640
        );
        let f = Feedback {
            seq: 1,
            transfer_ms: u32::MAX,
            cpu: Some(255),
            memory: Some(255),
        }
        .link();
        assert_eq!(f.transfer_ms, 7000);
        assert_eq!(f.cpu, Some(100));
        assert_eq!(f.memory, Some(100));
    }
    #[test]
    fn slowest_receiver_sets_budget_and_slow_links_do_not_build_a_frame_queue() {
        let start = Instant::now();
        let mut c = Controller::at(start);
        let link = Link::worst(
            [
                fast().unwrap(),
                Link {
                    transfer_ms: 2500,
                    cpu: Some(95),
                    memory: Some(60),
                },
            ]
            .into_iter(),
        );
        let plan = c.plan(
            start,
            true,
            false,
            Load {
                available_mib: Some(100),
                ..idle()
            },
            link,
        );
        assert_eq!(plan.width, 640);
        assert_eq!(plan.interval_ms, 3750);
        assert_eq!(link.unwrap().cpu, Some(95));
    }
    #[test]
    fn sustained_headroom_reaches_architecture_limit() {
        let start = Instant::now();
        let mut c = Controller::at(start);
        for second in 0..50 {
            c.capture(Duration::from_millis(10), false);
            c.plan(
                start + Duration::from_secs(second),
                true,
                false,
                idle(),
                fast(),
            );
        }
        assert_eq!(c.status.width, if usize::BITS == 32 { 1280 } else { 1920 });
    }
}
