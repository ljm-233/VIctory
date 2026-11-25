//! What the run is using of the machine, for a line a second in the log: the
//! processor cores of this process, its memory, the card's engines and the
//! card's memory.
//!
//! Processor and memory are the kernel's own accounting (`GetProcessTimes`,
//! `K32GetProcessMemoryInfo`); the card's engines come from the system's
//! "GPU Engine" performance counters, per process id, so the percentages are
//! this run's and not another program's.


use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
use windows_sys::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW,
    PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
};

/// What the run uses at one look.
#[derive(Debug, Default, Clone)]
pub(crate) struct Sample {
    /// Processor cores used since the last look: 1.0 is one logical processor
    /// busy for the whole interval.
    pub cores: Option<f64>,
    /// The logical processors the machine has.
    pub logical: u64,
    /// Bytes of physical memory this process works in.
    pub ram: u64,
    /// Bytes of this process's memory the system counts as committed.
    pub commit: u64,
    /// The card's engines this process used, in percent of the interval: all
    /// of them, then 3D and compute (copy and video are in the first only).
    pub gpu: Option<f64>,
    pub gpu_3d: Option<f64>,
    pub gpu_compute: Option<f64>,
}

static LAST: std::sync::Mutex<Option<(std::time::Instant, u64)>> = std::sync::Mutex::new(None);
static GPU: std::sync::Mutex<Option<GpuCounters>> = std::sync::Mutex::new(None);

fn hundred_ns(t: &FILETIME) -> u64 {
    u64::from(t.dwHighDateTime) << 32 | u64::from(t.dwLowDateTime)
}

/// `kernel + user` of this process, in 100 ns units.
fn process_cpu() -> Option<u64> {
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: four locals the kernel fills; our own process handle.
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    };
    (ok != 0).then(|| hundred_ns(&kernel) + hundred_ns(&user))
}

/// This process's working set and committed bytes.
fn process_memory() -> (u64, u64) {
    let mut c = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    // SAFETY: a valid out structure of the size it says.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    if ok == 0 {
        return (0, 0);
    }
    (c.WorkingSetSize as u64, c.PagefileUsage as u64)
}

/// The system's engine counters for this process, opened once.
struct GpuCounters {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
}

// SAFETY: the handles are the PDH query's own, used behind the map's lock.
unsafe impl Send for GpuCounters {}

impl GpuCounters {
    fn open() -> Option<Self> {
        let mut query: PDH_HQUERY = std::ptr::null_mut();
        // SAFETY: the API fills the handles; every one made is closed in Drop.
        unsafe {
            if PdhOpenQueryW(std::ptr::null(), 0, &mut query) != 0 {
                return None;
            }
            let path: Vec<u16> = "\\GPU Engine(*)\\Utilization Percentage\0"
                .encode_utf16()
                .collect();
            let mut counter: PDH_HCOUNTER = std::ptr::null_mut();
            if PdhAddEnglishCounterW(query, path.as_ptr(), 0, &mut counter) != 0 {
                PdhCloseQuery(query);
                return None;
            }
            // A rate counter needs two reads before it says anything; the
            // first is taken here so the run's first line already has a base.
            PdhCollectQueryData(query);
            Some(GpuCounters { query, counter })
        }
    }

    /// The engine percentages for `pid`, as `(all, 3D, compute)`.
    fn read(&self, pid: u32) -> Option<(f64, f64, f64)> {
        // SAFETY: the query is ours; the array is sized by the API's answer.
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return None;
            }
            let mut size = 0u32;
            let mut count = 0u32;
            let more = PdhGetFormattedCounterArrayW(
                self.counter,
                PDH_FMT_DOUBLE,
                &mut size,
                &mut count,
                std::ptr::null_mut(),
            );
            // PDH_MORE_DATA: the size the buffer needs. Anything else is no
            // answer at all (a counter that has no instances yet).
            if more as u32 != 0x8000_07D2 {
                return None;
            }
            let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
            let items = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            if PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, items)
                as u32
                != 0
            {
                return None;
            }
            let own = format!("pid_{pid}_");
            let mut all = 0.0;
            let mut three = 0.0;
            let mut compute = 0.0;
            for i in 0..count as usize {
                let item = &*items.add(i);
                if item.szName.is_null() {
                    continue;
                }
                let mut len = 0;
                while *item.szName.add(len) != 0 {
                    len += 1;
                }
                let name = String::from_utf16_lossy(std::slice::from_raw_parts(item.szName, len));
                if !name.contains(&own) {
                    continue;
                }
                let value = match item.FmtValue.Anonymous.doubleValue {
                    v if v.is_finite() => v,
                    _ => continue,
                };
                // The engines a game drives: 3D, compute and the copies. Summed
                // the way the system's own "GPU Engine" set is: each instance is
                // its own percentage.
                if std::env::var_os("VICTORY_USAGE_NAMES").is_some() && value > 0.0 {
                    eprintln!("GPU ENGINE {name} = {value}");
                }
                if name.contains("engtype_3D") {
                    all += value;
                    three += value;
                } else if name.contains("engtype_Compute") {
                    all += value;
                    compute += value;
                } else if name.contains("engtype_Copy") {
                    all += value;
                }
            }
            Some((all, three, compute))
        }
    }
}

impl Drop for GpuCounters {
    fn drop(&mut self) {
        // SAFETY: our own query, closed once.
        unsafe { PdhCloseQuery(self.query) };
    }
}

/// One look at what the run is using.
pub(crate) fn sample() -> Sample {
    let mut out = Sample {
        logical: std::thread::available_parallelism().map_or(1, |n| n.get() as u64),
        ..Sample::default()
    };
    let now = std::time::Instant::now();
    if let Some(cpu) = process_cpu() {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((then, before)) = *last {
            let wall = now.duration_since(then).as_secs_f64();
            if wall > 0.0 {
                out.cores = Some(cpu.saturating_sub(before) as f64 / 1e7 / wall);
            }
        }
        *last = Some((now, cpu));
    }
    (out.ram, out.commit) = process_memory();
    let mut gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    if gpu.is_none() {
        *gpu = GpuCounters::open();
    }
    if let Some(counters) = gpu.as_ref()
        && let Some((all, three, compute)) = counters.read(std::process::id())
    {
        out.gpu = Some(all);
        out.gpu_3d = Some(three);
        out.gpu_compute = Some(compute);
    }
    out
}
