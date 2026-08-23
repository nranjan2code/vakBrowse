//! Stealth: deterministic per-identity fingerprint profiles, the JS patch
//! bundle injected before every document, and humanized pointer paths.
//!
//! Honest scope: this defeats naive automation detection
//! (navigator.webdriver, headless UA leaks, missing plugin/language data,
//! robotic single-jump mouse teleports). It does not defeat sophisticated
//! behavioral biometrics or TLS fingerprinting.

use serde::{Deserialize, Serialize};
use vakbrowse_core::Result;
use std::fmt::Write as _;

/// A coherent hardware/locale identity derived deterministically from a
/// seed (e.g. the profile id) so an agent's sessions look like the same
/// machine every run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StealthProfile {
    pub seed: String,
    pub user_agent: String,
    pub platform: String,
    pub languages: Vec<String>,
    pub hardware_concurrency: u32,
    pub device_memory: u32,
    pub timezone_id: String,
}

impl StealthProfile {
    /// FNV-1a based deterministic derivation — stable across runs/platforms.
    pub fn generate(seed: &str) -> Self {
        let h = fnv1a(seed.as_bytes());
        let mac = h.is_multiple_of(2); // alternate between macOS and Windows personas

        let (platform, ua_platform) = if mac {
            ("MacIntel", "Macintosh; Intel Mac OS X 10_15_7")
        } else {
            ("Win32", "Windows NT 10.0; Win64; x64")
        };
        let chrome_major = 130 + (h >> 8) % 20; // recent-ish, stable per seed
        let languages = if h >> 4 & 1 == 0 {
            vec!["en-US".into(), "en".into()]
        } else {
            vec!["en-GB".into(), "en".into()]
        };

        Self {
            seed: seed.to_string(),
            user_agent: format!(
                "Mozilla/5.0 ({ua_platform}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{chrome_major}.0.0.0 Safari/537.36"
            ),
            platform: platform.into(),
            languages,
            hardware_concurrency: 4 + ((h >> 12) % 13) as u32, // 4..=16
            device_memory: match (h >> 16) % 4 {
                0 => 4,
                1 => 8,
                _ => 16,
            },
            timezone_id: if h >> 20 & 1 == 0 {
                "America/New_York".into()
            } else {
                "America/Los_Angeles".into()
            },
        }
    }

    /// JS bundle registered via `Page.addScriptToEvaluateOnNewDocument`,
    /// so patches land before any page script runs.
    pub fn init_script(&self) -> String {
        let langs = self
            .languages
            .iter()
            .map(|l| format!("'{l}'"))
            .collect::<Vec<_>>()
            .join(",");
        let mut js = String::with_capacity(2048);
        let _ = write!(
            js,
            r#"(() => {{
  const defineProp = (obj, name, value) => {{
    Object.defineProperty(obj, name, {{ get: () => value, configurable: true }});
  }};
  defineProp(navigator, 'webdriver', false);
  defineProp(navigator, 'platform', '{platform}');
  defineProp(navigator, 'hardwareConcurrency', {cores});
  defineProp(navigator, 'deviceMemory', {mem});
  defineProp(navigator, 'languages', Object.freeze([{langs}]));
  defineProp(navigator, 'language', '{lang0}');
  if (!window.chrome) {{ window.chrome = {{ runtime: {{}} }}; }}
}})();
"#,
            platform = self.platform,
            cores = self.hardware_concurrency,
            mem = self.device_memory,
            langs = langs,
            lang0 = self.languages[0],
        );
        js
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Humanized pointer path: cubic-bezier from `from` to `to` with two
/// control points jittered by a seeded PRNG, sampled into discrete steps.
pub fn mouse_path(from: (f64, f64), to: (f64, f64), seed: u64, steps: usize) -> Vec<(f64, f64)> {
    let steps = steps.max(4);
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    // Control points perpendicular-ish to the line, offset by noise.
    let n = |i: u64| -> f64 {
        let x = fnv1a(&[(seed >> i) as u8, (seed >> (i + 8)) as u8, i as u8]);
        (x % 200) as f64 / 100.0 - 1.0 // -1..1
    };
    let c1 = (from.0 + dx * 0.25 + n(0) * 40.0, from.1 + dy * 0.25 + n(1) * 40.0);
    let c2 = (from.0 + dx * 0.75 + n(2) * 30.0, from.1 + dy * 0.75 + n(3) * 30.0);

    let bez = |t: f64| -> (f64, f64) {
        let mt = 1.0 - t;
        (
            mt * mt * mt * from.0 + 3.0 * mt * mt * t * c1.0 + 3.0 * mt * t * t * c2.0 + t * t * t * to.0,
            mt * mt * mt * from.1 + 3.0 * mt * mt * t * c1.1 + 3.0 * mt * t * t * c2.1 + t * t * t * to.1,
        )
    };
    (0..=steps)
        .map(|i| {
            let t = i as f64 / steps as f64;
            // ease-out: fast start, gentle landing
            let te = 1.0 - (1.0 - t).powi(2);
            bez(te)
        })
        .collect()
}

/// Default hardened launch flags for stealthy runs.
pub const STEALTH_ARGS: &[&str] = &[
    "--disable-blink-features=AutomationControlled",
    "--disable-features=IsolateOrigins,site-per-process",
    "--no-default-browser-check",
    "--start-maximized",
];

pub fn validate_profile(profile: &StealthProfile) -> Result<()> {
    if profile.hardware_concurrency == 0 || profile.device_memory == 0 {
        return Err(vakbrowse_core::VakError::Engine(
            "stealth profile has impossible hardware values".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_are_deterministic_per_seed() {
        let a = StealthProfile::generate("agent-1");
        let b = StealthProfile::generate("agent-1");
        assert_eq!(a, b);
        assert!(!a.user_agent.is_empty());
        validate_profile(&a).unwrap();
    }

    #[test]
    fn init_script_patches_webdriver_and_languages() {
        let p = StealthProfile::generate("seed-x");
        let js = p.init_script();
        assert!(js.contains("'webdriver', false"));
        assert!(js.contains(p.platform.trim_end_matches('3').trim()) || js.contains(&p.platform));
        assert!(js.contains(&format!("'{}'", p.languages[0])));
        assert!(js.contains("window.chrome"));
    }

    #[test]
    fn mouse_path_starts_ends_correctly_and_is_smooth() {
        let path = mouse_path((10.0, 10.0), (300.0, 400.0), 42, 24);
        assert_eq!(path.len(), 25);
        let (sx, sy) = path[0];
        let (ex, ey) = *path.last().unwrap();
        assert!((sx - 10.0).abs() < 1e-6 && (sy - 10.0).abs() < 1e-6);
        assert!((ex - 300.0).abs() < 1e-6 && (ey - 400.0).abs() < 1e-6);
        // No teleporting: consecutive points stay within a plausible step.
        for w in path.windows(2) {
            let d = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
            assert!(d < 120.0, "step too large: {d}");
        }
    }
}
