//! Read-only JSON I/O comparison. Prints timings/counts, never journal contents.
//! Without --imports, use an independent synthetic manifest in a temporary directory.
use std::{
    cell::Cell,
    fs::{self, File},
    io::{self, BufReader, Read},
    path::PathBuf,
    time::{Duration, Instant},
};

struct CountReads<'a> {
    file: File,
    calls: &'a Cell<usize>,
}
impl Read for CountReads<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.calls.set(self.calls.get() + 1);
        self.file.read(bytes)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    let fixture = tempfile::tempdir()?;
    let directory = if let Some(index) = args.iter().position(|arg| arg == "--imports") {
        PathBuf::from(args.get(index + 1).ok_or("Missing --imports directory")?)
    } else {
        let entries = (0..40000)
            .map(|index| {
                serde_json::json!({
                    "path": format!("game/images/资源/scene-{index:06}.webp"),
                    "directory": false, "bytes": 123456, "modified": 1700000000000u64
                })
            })
            .collect::<Vec<_>>();
        fs::write(
            fixture.path().join("fixture.manifest"),
            serde_json::to_vec(&entries)?,
        )?;
        fixture.path().to_owned()
    };
    if !directory.is_dir() {
        return Err("Pass an existing directory; real data is only read".into());
    }
    let mut paths = fs::read_dir(&directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    paths.retain(|path| {
        path.extension()
            .is_some_and(|ext| ext == "json" || ext == "manifest")
    });
    paths.sort();
    let (mut bytes, mut old_calls, mut buffered_calls) = (0, 0, 0);
    let (mut old_time, mut buffered_time) = (Duration::ZERO, Duration::ZERO);
    for path in &paths {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.len() > 256 * 1024 * 1024 {
            return Err("Only regular JSON files up to 256 MiB are measured".into());
        }
        bytes += metadata.len();
        // Warm the cache for both readers; discard bytes without exporting them.
        io::copy(&mut File::open(path)?, &mut io::sink())?;
        let calls = Cell::new(0);
        let started = Instant::now();
        let original: serde_json::Value = serde_json::from_reader(CountReads {
            file: File::open(path)?,
            calls: &calls,
        })?;
        old_time += started.elapsed();
        old_calls += calls.get();
        calls.set(0);
        let started = Instant::now();
        let buffered: serde_json::Value = serde_json::from_reader(BufReader::with_capacity(
            64 * 1024,
            CountReads {
                file: File::open(path)?,
                calls: &calls,
            },
        ))?;
        buffered_time += started.elapsed();
        buffered_calls += calls.get();
        if original != buffered {
            return Err("JSON changed between measurements; retry when imports are idle".into());
        }
    }
    println!(
        "files={} bytes={} unbuffered_ms={:.2} buffered_ms={:.2} unbuffered_reads={} buffered_reads={}",
        paths.len(), bytes, old_time.as_secs_f64() * 1000.0,
        buffered_time.as_secs_f64() * 1000.0, old_calls, buffered_calls,
    );
    Ok(())
}
