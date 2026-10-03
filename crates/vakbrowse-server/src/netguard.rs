//! Private-network guard. Every browser connection is routed through a
//! loopback HTTP proxy that resolves the destination itself, refuses
//! private/internal addresses, and connects to the exact IP it checked.
//!
//! Why a proxy and not a URL check: a URL check only sees the top-level
//! request. Redirects, iframes, subresources, `fetch()`, workers and
//! WebSockets all go through Chrome's proxy path, so this covers them, and
//! because the proxy connects to the address it screened, DNS rebinding
//! (resolve public, connect private) cannot slip between check and use.
//! WebRTC UDP, which bypasses proxies, is disabled while the guard is on.

use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const MAX_HEAD_BYTES: usize = 64 * 1024;
const HEAD_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const RECENT_BLOCKS: usize = 32;

/// First words of the guard's block page; the manager recognises a main
/// document that *is* the block page by it.
pub const BLOCK_PAGE_PREFIX: &str = "vakBrowse blocked this request";

/// Hosts the guard lets through even though they are private (local dev
/// servers). Entries: `host`, `host:port`, `[v6]:port` or a full URL.
#[derive(Debug, Clone, Default)]
pub struct GuardRules {
    allow: Vec<(String, Option<u16>)>,
    /// Screened answers, shared by every connection: Chrome caches DNS but
    /// the proxy resolves per connection, so without this a slow or flaky
    /// resolver stalls every subresource. Only allowed answers are kept, and
    /// connections still go to exactly these addresses.
    dns: Arc<Mutex<DnsCache>>,
}

type DnsCache = HashMap<(String, u16), (Instant, Vec<SocketAddr>)>;

const DNS_TTL: Duration = Duration::from_secs(30);
const DNS_CACHE_MAX: usize = 1024;

impl GuardRules {
    pub fn new(entries: &[String]) -> Self {
        Self {
            allow: entries.iter().filter_map(|e| parse_allow(e)).collect(),
            dns: Arc::default(),
        }
    }

    fn cached(&self, name: &str, port: u16) -> Option<Vec<SocketAddr>> {
        let map = self.dns.lock().ok()?;
        map.get(&(name.to_string(), port))
            .filter(|(at, _)| at.elapsed() < DNS_TTL)
            .map(|(_, a)| a.clone())
    }

    fn remember(&self, name: &str, port: u16, addrs: &[SocketAddr]) {
        if let Ok(mut map) = self.dns.lock() {
            if map.len() >= DNS_CACHE_MAX {
                map.retain(|_, (at, _)| at.elapsed() < DNS_TTL);
                if map.len() >= DNS_CACHE_MAX {
                    map.clear();
                }
            }
            map.insert((name.to_string(), port), (Instant::now(), addrs.to_vec()));
        }
    }

    fn exempt(&self, host: &str, port: u16) -> bool {
        let host = normalize_host(host);
        self.allow
            .iter()
            .any(|(h, p)| *h == host && p.is_none_or(|p| p == port))
    }
}

fn normalize_host(host: &str) -> String {
    host.trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn parse_allow(entry: &str) -> Option<(String, Option<u16>)> {
    let entry = entry.trim();
    if entry.is_empty() {
        return None;
    }
    if entry.contains("://") {
        let u = url::Url::parse(entry).ok()?;
        return Some((normalize_host(u.host_str()?), u.port_or_known_default()));
    }
    match split_host_port(entry) {
        Some((h, p)) => Some((normalize_host(&h), Some(p))),
        None => Some((normalize_host(entry), None)),
    }
}

/// `host:port` / `[v6]:port` → parts; None when there is no port.
fn split_host_port(s: &str) -> Option<(String, u16)> {
    if let Some(rest) = s.strip_prefix('[') {
        let (h, tail) = rest.split_once(']')?;
        return Some((h.to_string(), tail.strip_prefix(':')?.parse().ok()?));
    }
    let (h, p) = s.rsplit_once(':')?;
    if h.contains(':') {
        return None; // bare IPv6 without brackets
    }
    Some((h.to_string(), p.parse().ok()?))
}

/// True for every address an agent's browser must not reach by default:
/// loopback, private, link-local (incl. cloud metadata), CGNAT, multicast,
/// reserved and unspecified ranges, in IPv4 and IPv6 (incl. mapped forms).
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => blocked_v4(v4),
        IpAddr::V6(v6) => blocked_v6(v6),
    }
}

fn blocked_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0
        || (a == 100 && (64..128).contains(&b)) // CGNAT 100.64/10
        || (a == 192 && b == 0 && c == 0) // IETF protocol assignments
        || (a == 198 && (b == 18 || b == 19)) // benchmarking 198.18/15
        || a >= 240 // reserved
}

fn blocked_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return blocked_v4(v4);
    }
    let seg = ip.segments();
    // NAT64 well-known prefix 64:ff9b::/96 embeds an IPv4 address.
    if seg[0] == 0x64 && seg[1] == 0xff9b && seg[2..6] == [0, 0, 0, 0] {
        let v4 = Ipv4Addr::new(
            (seg[6] >> 8) as u8,
            seg[6] as u8,
            (seg[7] >> 8) as u8,
            seg[7] as u8,
        );
        return blocked_v4(v4);
    }
    ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (seg[0] & 0xfe00) == 0xfc00 // unique local fc00::/7
        || (seg[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
        || (seg[0] & 0xffc0) == 0xfec0 // site-local (deprecated)
        || (seg[0] == 0x2001 && seg[1] == 0x0db8) // documentation
        || seg[..6] == [0, 0, 0, 0, 0, 0] // IPv4-compatible ::a.b.c.d
}

fn is_local_name(host: &str) -> bool {
    host == "localhost" || host.ends_with(".localhost")
}

/// Resolve `host:port` and return the addresses to try, or why the
/// destination is refused. A name resolving to ANY private address is
/// refused (a mixed answer is the classic rebinding setup).
pub async fn resolve_checked(
    host: &str,
    port: u16,
    rules: &GuardRules,
) -> std::result::Result<Vec<SocketAddr>, String> {
    let name = normalize_host(host);
    if name.is_empty() {
        return Err("empty host".into());
    }
    let exempt = rules.exempt(&name, port);
    if let Ok(ip) = name.parse::<IpAddr>() {
        if !exempt && is_blocked_ip(ip) {
            return Err(format!("{name} is a private/internal address"));
        }
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    if !exempt && is_local_name(&name) {
        return Err(format!("{name} is a local host name"));
    }
    if let Some(hit) = rules.cached(&name, port) {
        return Ok(hit);
    }
    let addrs: Vec<SocketAddr> = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::net::lookup_host((name.as_str(), port)),
    )
    .await
    .map_err(|_| format!("dns timeout for {name}"))?
    .map_err(|e| format!("dns failure for {name}: {e}"))?
    .collect();
    if addrs.is_empty() {
        return Err(format!("{name} did not resolve"));
    }
    if !exempt && let Some(bad) = addrs.iter().find(|a| is_blocked_ip(a.ip())) {
        return Err(format!(
            "{name} resolves to private/internal address {}",
            bad.ip()
        ));
    }
    rules.remember(&name, port, &addrs);
    Ok(addrs)
}

/// The running guard proxy. Dropping it stops accepting connections.
pub struct NetGuard {
    addr: SocketAddr,
    rules: Arc<GuardRules>,
    blocked: Arc<Mutex<VecDeque<(Instant, String)>>>,
    blocks: Arc<AtomicU64>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for NetGuard {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl NetGuard {
    pub async fn start(rules: GuardRules) -> std::io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let addr = listener.local_addr()?;
        let rules = Arc::new(rules);
        let blocked = Arc::new(Mutex::new(VecDeque::new()));
        let blocks = Arc::new(AtomicU64::new(0));
        let (r, b, n) = (rules.clone(), blocked.clone(), blocks.clone());
        let task = tokio::spawn(async move {
            loop {
                let Ok((client, _)) = listener.accept().await else {
                    continue;
                };
                let (r, b, n) = (r.clone(), b.clone(), n.clone());
                tokio::spawn(async move {
                    let _ = serve(client, &r, &b, &n).await;
                });
            }
        });
        tracing::info!(%addr, "private-network guard listening");
        Ok(Self {
            addr,
            rules,
            blocked,
            blocks,
            task,
        })
    }

    /// `--proxy-server` value for Chrome.
    pub fn proxy_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Extra Chrome args that make every connection use the guard: by
    /// default Chrome bypasses proxies for loopback, and WebRTC UDP ignores
    /// them entirely.
    pub fn chrome_args() -> Vec<String> {
        vec![
            "--proxy-bypass-list=<-loopback>".into(),
            "--force-webrtc-ip-handling-policy=disable_non_proxied_udp".into(),
        ]
    }

    /// Pre-flight check for a top-level URL, so the agent gets a clear
    /// policy error instead of a browser error page.
    pub async fn check_url(&self, raw: &str) -> std::result::Result<(), String> {
        let Ok(u) = url::Url::parse(raw) else {
            return Ok(());
        };
        if !matches!(u.scheme(), "http" | "https" | "ws" | "wss") {
            return Ok(());
        }
        let (Some(host), Some(port)) = (u.host_str(), u.port_or_known_default()) else {
            return Ok(());
        };
        resolve_checked(host, port, &self.rules).await.map(|_| ())
    }

    /// Total connections refused so far (to tell whether an action hit it).
    pub fn block_count(&self) -> u64 {
        self.blocks.load(Ordering::Relaxed)
    }

    /// Most recent block within `within`, used to explain a navigation that
    /// ended on a browser error page (e.g. a redirect into a private range).
    pub fn recent_block(&self, within: Duration) -> Option<String> {
        let q = self.blocked.lock().ok()?;
        q.back()
            .filter(|(at, _)| at.elapsed() <= within)
            .map(|(_, why)| why.clone())
    }
}

async fn serve(
    mut client: TcpStream,
    rules: &GuardRules,
    blocked: &Mutex<VecDeque<(Instant, String)>>,
    blocks: &AtomicU64,
) -> std::io::Result<()> {
    let mut buf = Vec::with_capacity(4096);
    let head_end = tokio::time::timeout(HEAD_TIMEOUT, read_head(&mut client, &mut buf))
        .await
        .map_err(|_| std::io::Error::other("head timeout"))??;
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.split("\r\n");
    let first = lines.next().unwrap_or_default();
    let mut parts = first.split_whitespace();
    let (Some(method), Some(target), Some(version)) = (parts.next(), parts.next(), parts.next())
    else {
        return reply(&mut client, "400 Bad Request", "malformed request").await;
    };

    let (host, port, rewritten) = if method.eq_ignore_ascii_case("CONNECT") {
        let Some((h, p)) = split_host_port(target) else {
            return reply(&mut client, "400 Bad Request", "bad CONNECT target").await;
        };
        (h, p, None)
    } else {
        let Ok(u) = url::Url::parse(target) else {
            return reply(
                &mut client,
                "400 Bad Request",
                "proxy requests need an absolute URL",
            )
            .await;
        };
        let (Some(h), Some(p)) = (u.host_str(), u.port_or_known_default()) else {
            return reply(&mut client, "400 Bad Request", "missing host").await;
        };
        let mut path = u.path().to_string();
        if let Some(q) = u.query() {
            path.push('?');
            path.push_str(q);
        }
        // Origin-form request line; hop-by-hop proxy headers dropped, and
        // the connection closed after one exchange so it can never be
        // reused for a different (unchecked) host.
        let mut out = format!("{method} {path} {version}\r\n");
        for line in lines.filter(|l| !l.is_empty()) {
            let name = line.split(':').next().unwrap_or_default().trim();
            if [
                "proxy-connection",
                "proxy-authorization",
                "connection",
                "keep-alive",
            ]
            .iter()
            .any(|h| name.eq_ignore_ascii_case(h))
            {
                continue;
            }
            out.push_str(line);
            out.push_str("\r\n");
        }
        out.push_str("Connection: close\r\n\r\n");
        (h.to_string(), p, Some(out))
    };

    let addrs = match resolve_checked(&host, port, rules).await {
        Ok(a) => a,
        Err(why) => {
            tracing::warn!(%host, port, %why, "guard blocked connection");
            blocks.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut q) = blocked.lock() {
                if q.len() == RECENT_BLOCKS {
                    q.pop_front();
                }
                q.push_back((Instant::now(), why.clone()));
            }
            return reply(&mut client, "403 Forbidden", &format!(
                "{BLOCK_PAGE_PREFIX}: {why}. Allow a local dev host with VAKBROWSE_ALLOW_PRIVATE_HOSTS."
            ))
            .await;
        }
    };
    // Try each screened address in turn: an AAAA record on a host without
    // IPv6 must not fail a request that its A record would serve.
    let mut upstream = None;
    for addr in &addrs {
        if let Ok(Ok(s)) = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(addr)).await {
            upstream = Some(s);
            break;
        }
    }
    let Some(mut upstream) = upstream else {
        return reply(&mut client, "502 Bad Gateway", "upstream connect failed").await;
    };
    let leftover = &buf[head_end..];
    match rewritten {
        None => {
            client
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await?;
        }
        Some(head) => upstream.write_all(head.as_bytes()).await?,
    }
    if !leftover.is_empty() {
        upstream.write_all(leftover).await?;
    }
    let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
    Ok(())
}

/// Read until the end of the request head; returns its length (incl. CRLFCRLF).
async fn read_head(client: &mut TcpStream, buf: &mut Vec<u8>) -> std::io::Result<usize> {
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            return Ok(i + 4);
        }
        if buf.len() > MAX_HEAD_BYTES {
            return Err(std::io::Error::other("request head too large"));
        }
        let n = client.read(&mut chunk).await?;
        if n == 0 {
            return Err(std::io::Error::other("client closed"));
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

async fn reply(client: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    let msg = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nX-VakBrowse-Guard: blocked\r\n\r\n{body}",
        body.len()
    );
    client.write_all(msg.as_bytes()).await?;
    client.shutdown().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_internal_ranges() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "224.0.0.1",
            "255.255.255.255",
            "::1",
            "::",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "64:ff9b::a9fe:a9fe",
        ] {
            assert!(is_blocked_ip(ip.parse().unwrap()), "{ip} should be blocked");
        }
        for ip in [
            "93.184.215.14",
            "1.1.1.1",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(!is_blocked_ip(ip.parse().unwrap()), "{ip} should pass");
        }
    }

    #[test]
    fn allow_entries_parse_and_match() {
        let r = GuardRules::new(&[
            "localhost:3000".into(),
            "http://127.0.0.1:8080/app".into(),
            "devbox".into(),
            "[::1]:5173".into(),
        ]);
        assert!(r.exempt("localhost", 3000));
        assert!(!r.exempt("localhost", 3001));
        assert!(r.exempt("127.0.0.1", 8080));
        assert!(r.exempt("DEVBOX", 1));
        assert!(r.exempt("[::1]", 5173));
    }

    #[tokio::test]
    async fn resolve_refuses_private_and_local_names() {
        let rules = GuardRules::default();
        assert!(resolve_checked("127.0.0.1", 80, &rules).await.is_err());
        assert!(resolve_checked("localhost", 80, &rules).await.is_err());
        assert!(resolve_checked("app.localhost", 80, &rules).await.is_err());
        assert!(resolve_checked("[::1]", 80, &rules).await.is_err());
        let open = GuardRules::new(&["127.0.0.1:80".into()]);
        assert!(resolve_checked("127.0.0.1", 80, &open).await.is_ok());
        // Allowed answers are cached and served from the cache.
        let fake = vec!["93.184.215.14:443".parse().unwrap()];
        rules.remember("cached.example", 443, &fake);
        assert_eq!(
            resolve_checked("cached.example", 443, &rules)
                .await
                .unwrap(),
            fake
        );
    }

    /// End to end through the proxy: a loopback origin is refused for both
    /// plain-HTTP and CONNECT, and an exempt one is served.
    #[tokio::test]
    async fn proxy_blocks_then_allows_exempt_origin() {
        let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let oaddr = origin.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = origin.accept().await {
                tokio::spawn(async move {
                    let mut b = [0u8; 1024];
                    let _ = s.read(&mut b).await;
                    let _ = s
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\nsecret")
                        .await;
                });
            }
        });

        async fn ask(guard: &NetGuard, req: String) -> String {
            let mut s = TcpStream::connect(guard.addr).await.unwrap();
            s.write_all(req.as_bytes()).await.unwrap();
            let mut out = String::new();
            let _ = s.read_to_string(&mut out).await;
            out
        }

        let closed = NetGuard::start(GuardRules::default()).await.unwrap();
        let get = format!("GET http://{oaddr}/ HTTP/1.1\r\nHost: {oaddr}\r\n\r\n");
        let out = ask(&closed, get.clone()).await;
        assert!(out.starts_with("HTTP/1.1 403"), "{out}");
        assert!(!out.contains("secret"));
        let conn = format!("CONNECT {oaddr} HTTP/1.1\r\nHost: {oaddr}\r\n\r\n");
        assert!(ask(&closed, conn).await.starts_with("HTTP/1.1 403"));
        assert!(closed.recent_block(Duration::from_secs(5)).is_some());

        let open = NetGuard::start(GuardRules::new(&[oaddr.to_string()]))
            .await
            .unwrap();
        let out = ask(&open, get).await;
        assert!(out.contains("secret"), "{out}");
    }
}
