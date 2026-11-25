//! The offline warm-up: every shader found is translated into the run's
//! shader cache ahead of time, so that a run does not translate while it
//! plays. Only the shader cache is filled: no pipeline is compiled. The
//! host device is still probed for its wave size and view-heap limit,
//! because those are part of the cache key.
//!
//! The key is the run's: the same executable (or translator, under
//! `VICTORY_SHADER_CACHE_KEY=translator`), the same GPU limits and the same
//! switches. `VICTORY_HOST_WAVE`/`VICTORY_VIEW_LIMIT` say what the run's log
//! says of the host (`host wave lanes`, `view heaps hold`), or the run probes
//! it from the device itself; the two must agree, or every entry misses.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::Result;

use crate::shader::{MAX_FILE, Source, sources};

/// The last milestone a thread printed, so each is printed once.
static PRINTED: AtomicUsize = AtomicUsize::new(0);

#[derive(Default)]
struct Tally {
    containers: usize,
    duplicates: usize,
    cached: usize,
    translated: usize,
    refused: usize,
    /// Refusal reason -> how many, and the first container it happened to.
    failures: BTreeMap<String, (usize, String)>,
    list: bool,
    lines: Vec<String>,
}

impl Tally {
    fn fail(&mut self, reason: String, example: impl FnOnce() -> String) {
        let e = example();
        if self.list {
            self.lines.push(format!("{e}  {reason}"));
        }
        self.failures.entry(reason).or_insert((0, e)).0 += 1;
    }

    fn merge(&mut self, other: Tally) {
        self.containers += other.containers;
        self.duplicates += other.duplicates;
        self.cached += other.cached;
        self.translated += other.translated;
        self.refused += other.refused;
        self.lines.extend(other.lines);
        for (reason, (n, example)) in other.failures {
            self.failures.entry(reason).or_insert((0, example)).0 += n;
        }
    }
}

pub fn run(paths: Vec<PathBuf>, jobs: Option<usize>, limit: Option<usize>, list: bool) -> Result<()> {
    let paths = crate::default_paths(paths)?;
    let cfg = victory_core::Config::load()?;
    let dir = cfg.cache_subdir("shaders")?;
    victory_hle::modules::d3d12x::use_shader_cache(dir.clone());
    println!("shader cache {}", dir.display());
    if std::env::var_os("VICTORY_SHADER_CACHE_KEY").is_none() {
        println!(
            "key          the executable's own hash (VICTORY_SHADER_CACHE_KEY=translator keeps \
             it across a rebuild of everything but victory-shader)"
        );
    }

    let threads = jobs
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get()))
        .max(1);
    let most = limit.unwrap_or(usize::MAX);
    let seen: Mutex<HashSet<blake3::Hash>> = Mutex::new(HashSet::new());
    let total: Mutex<Tally> = Mutex::new(Tally::default());
    let done = AtomicUsize::new(0);
    let began = Instant::now();

    for root in &paths {
        for entry in walkdir::WalkDir::new(root).into_iter().filter_map(Result::ok) {
            if done.load(Ordering::Relaxed) >= most {
                break;
            }
            if !entry.file_type().is_file() || entry.metadata().map_or(0, |m| m.len()) > MAX_FILE {
                continue;
            }
            let Ok(data) = std::fs::read(entry.path()) else {
                continue;
            };
            let jobs_here = sources(&data);
            if jobs_here.is_empty() {
                continue;
            }
            let next = AtomicUsize::new(0);
            std::thread::scope(|s| {
                for _ in 0..threads.min(jobs_here.len()) {
                    s.spawn(|| {
                        let mut t = Tally {
                            list,
                            ..Tally::default()
                        };
                        while let Some(job) = jobs_here.get(next.fetch_add(1, Ordering::Relaxed)) {
                            if done.fetch_add(1, Ordering::Relaxed) >= most {
                                break;
                            }
                            warm(job, entry.path(), &seen, &mut t);
                            // One progress line per 1,000 containers, from
                            // whichever thread got the counter there.
                            let n = done.load(Ordering::Relaxed);
                            if n.is_multiple_of(1000) && n > 0 && PRINTED.swap(n, Ordering::Relaxed) != n
                            {
                                println!(
                                    "warm-up      {n} shaders so far ({:.0} s)",
                                    began.elapsed().as_secs_f64()
                                );
                            }
                        }
                        lock(&total).merge(t);
                    });
                }
            });
        }
    }

    let t = total.into_inner().unwrap_or_else(|e| e.into_inner());
    let mut lines = t.lines;
    lines.sort();
    for l in lines {
        println!("{l}");
    }
    println!(
        "shaders      {} translated now, {} already cached, {} refused ({:.1} s, {threads} threads)",
        t.translated,
        t.cached,
        t.refused,
        began.elapsed().as_secs_f64()
    );
    println!(
        "containers   {} unique ({} duplicates skipped)",
        t.containers, t.duplicates
    );
    if !t.failures.is_empty() {
        println!("refusals (top 15):");
        let mut failures: Vec<_> = t.failures.into_iter().collect();
        failures.sort_by_key(|(_, (n, _))| std::cmp::Reverse(*n));
        for (reason, (n, example)) in failures.into_iter().take(15) {
            println!("  {n:>7}  {reason}  (e.g. {example})");
        }
    }
    Ok(())
}

/// Translates one container into the cache, counting what it found.
fn warm(source: &Source, path: &Path, seen: &Mutex<HashSet<blake3::Hash>>, t: &mut Tally) {
    let Some(bytes) = source.bytes() else {
        return t.fail("corrupt record".into(), || {
            format!("{}{}", path.display(), source.label())
        });
    };
    let hash = blake3::hash(&bytes);
    if !lock(seen).insert(hash) {
        t.duplicates += 1;
        return;
    }
    t.containers += 1;
    let example = || {
        format!(
            "{}{} #{}",
            path.display(),
            source.label(),
            &hash.to_hex()[..16]
        )
    };
    match victory_hle::modules::d3d12x::warm_shader(&bytes) {
        Ok(true) => t.cached += 1,
        Ok(false) => {
            t.translated += 1;
            if t.list {
                t.lines.push(format!("{}  translated", example()));
            }
        }
        Err(reason) => {
            t.refused += 1;
            t.fail(reason, example);
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
