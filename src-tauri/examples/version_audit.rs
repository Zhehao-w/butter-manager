//! Audit a private JSON array of directory names. Prints aggregate counts only.
use butter_manager::version::simple_version;
use std::time::Instant;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Pass a JSON name list")?;
    let names: Vec<String> = serde_json::from_slice(&std::fs::read(path)?)?;
    let start = Instant::now();
    let (mut v, mut ver, mut bare) = (0, 0, 0);
    for name in &names {
        if let Some(version) = simple_version(name) {
            let value = version.to_ascii_lowercase();
            if value.starts_with("ver") {
                ver += 1;
            } else if value.starts_with('v') {
                v += 1;
            } else {
                bare += 1;
            }
        }
    }
    println!(
        "names={} recognized={} v={} ver={} bare={} unmatched={} elapsed_us={}",
        names.len(),
        v + ver + bare,
        v,
        ver,
        bare,
        names.len() - v - ver - bare,
        start.elapsed().as_micros()
    );
    Ok(())
}
