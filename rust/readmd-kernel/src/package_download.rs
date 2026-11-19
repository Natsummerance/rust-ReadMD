//! Choose a transport for bytes whose digest was obtained independently.
use std::io::Read;
use std::time::{Duration, Instant};

const PROBE_BYTES: usize = 64 * 1024;
const PROBE_TIMEOUT: Duration = Duration::from_millis(2400);

fn host(url: &str) -> Option<&str> {
    let (host, _) = url.strip_prefix("https://")?.split_once('/')?;
    (!url.chars().any(|c| c.is_control() || c == '\\')).then_some(host)
}

fn allowed_host(value: &str) -> bool {
    matches!(value, "github.com" | "release-assets.githubusercontent.com" |
        "objects.githubusercontent.com" | "github-releases.githubusercontent.com")
        || crate::updater::MIRROR_PREFIXES.iter().any(|prefix| host(prefix) == Some(value))
}

fn public_release(url: &str) -> bool {
    host(url) == Some("github.com") && !url.contains(['?', '#'])
        && url.contains("/releases/download/")
}

/// Redirects are checked before following them. No headers carrying user
/// credentials are sent to download accelerators.
pub(crate) fn response(
    agent: &ureq::Agent, url: &str, deadline: Instant, range: Option<&str>,
) -> Result<ureq::Response, ()> {
    let mut target = url.to_string();
    for _ in 0..=5 {
        let authority = host(&target).filter(|host| allowed_host(host)).ok_or(())?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() { return Err(()); }
        let mut request = agent.get(&target).timeout(remaining).set("Accept-Encoding", "identity");
        if let Some(range) = range { request = request.set("Range", range); }
        let reply = request.call().map_err(|_| ())?;
        match reply.status() {
            200 => return Ok(reply),
            206 if range.is_some() => return Ok(reply),
            301 | 302 | 303 | 307 | 308 => {
                let location = reply.header("Location").ok_or(())?;
                target = if location.starts_with('/') && !location.starts_with("//") {
                    format!("https://{authority}{location}")
                } else { location.to_string() };
            }
            _ => return Err(()),
        }
    }
    Err(())
}

pub(crate) fn ranked(official: &str, prefer_mirror: bool, size: Option<u64>) -> Vec<String> {
    if !public_release(official) { return vec![official.into()]; }
    let candidates = crate::updater::download_candidates(official, prefer_mirror);
    let deadline = Instant::now() + PROBE_TIMEOUT;
    let agent = crate::updater::update_agent_with_read_timeout(PROBE_TIMEOUT, 0, PROBE_TIMEOUT);
    rank_by(candidates, |url| {
        let start = Instant::now();
        let sample = size.unwrap_or(PROBE_BYTES as u64).min(PROBE_BYTES as u64) as usize;
        if sample == 0 { return None; }
        let range = format!("bytes=0-{}", sample - 1);
        let reply = response(&agent, url, deadline, Some(&range)).ok()?;
        let total = if reply.status() == 206 {
            let (span, total) = reply.header("Content-Range")?.strip_prefix("bytes ")?.split_once('/')?;
            let (begin, end) = span.split_once('-')?;
            if begin != "0" || end.parse::<u64>().ok()? < (sample - 1) as u64 { return None; }
            Some(total.parse::<u64>().ok()?)
        } else { reply.header("Content-Length").and_then(|value| value.parse::<u64>().ok()) };
        if total.zip(size).is_some_and(|(actual, expected)| actual != expected) { return None; }
        if let Some(kind) = reply.header("Content-Type") {
            if kind.to_ascii_lowercase().starts_with("text/html") { return None; }
        }
        let mut buffer = vec![0; total.map_or(sample, |total| total.min(sample as u64) as usize)];
        if buffer.is_empty() { return None; }
        reply.into_reader().read_exact(&mut buffer).ok()?;
        Some(start.elapsed())
    })
}

fn rank_by(candidates: Vec<String>, probe: impl Fn(&str) -> Option<Duration> + Sync) -> Vec<String> {
    let scores = std::thread::scope(|scope| {
        let jobs: Vec<_> = candidates.iter().map(|url| scope.spawn(|| probe(url))).collect();
        jobs.into_iter().map(|job| job.join().ok().flatten()).collect::<Vec<_>>()
    });
    let mut ordered = candidates.into_iter().zip(scores).collect::<Vec<_>>();
    ordered.sort_by_key(|(_, elapsed)| elapsed.unwrap_or(Duration::MAX));
    ordered.into_iter().map(|(url, _)| url).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortest_sample_wins_and_all_fallbacks_remain() {
        let urls = ["origin", "unavailable", "fast", "medium"].map(String::from).to_vec();
        assert_eq!(rank_by(urls, |url| match url {
            "origin" => Some(Duration::from_millis(180)),
            "fast" => Some(Duration::from_millis(30)),
            "medium" => Some(Duration::from_millis(90)),
            _ => None,
        }), ["fast", "medium", "origin", "unavailable"]);
    }

    #[test]
    fn only_public_release_paths_can_be_given_to_mirrors() {
        assert!(public_release("https://github.com/a/b/releases/download/v1/package.zip"));
        assert!(!public_release("https://github.com/a/b/releases/download/v1/package.zip?token=value"));
        assert!(!public_release("https://github.com@localhost/a/b/releases/download/v1/package.zip"));
        assert!(!allowed_host("localhost"));
    }
}
