//! The Kindle's Wi-Fi, only while the phone link is on, behind a firewall that lets
//! in nothing but the link from the local network.
//!
//! Wi-Fi is switched with the Kindle's own `lipc-set-prop com.lab126.cmd
//! wirelessEnable`; its Wi-Fi daemon joins a known network on its own, even with
//! Amazon's interface stopped. The firewall (iptables, filter table) is shut BEFORE
//! the Wi-Fi comes on ([`lockdown`]: loopback and DHCP only), then opened just for the
//! link ([`firewall`]):
//! - in: loopback, DHCP answers, and TCP 7700 from the local subnet;
//! - out: loopback, DHCP requests, and replies from port 7700 to the local subnet;
//! - everything else dropped (no DNS, no Amazon, no Internet); IPv6 switched off.
//!
//! The rules in force before are saved to /tmp and put back when the link stops, the
//! wallet locks, the Kindle sleeps or KindleWallet exits (KindleWallet.sh also puts
//! them back after a crash).

use std::{
    net::Ipv4Addr,
    process::Command,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub const IFACE: &str = "wlan0";
const SAVED_V4: &str = "/tmp/kindlewallet.iptables";
const SAVED_V6: &str = "/tmp/kindlewallet.ip6tables";
/// How long to wait for the Wi-Fi to join a network and get an address.
const JOIN_TIMEOUT: Duration = Duration::from_secs(45);

/// Bringing up and tearing down never overlap.
static NET: Mutex<()> = Mutex::new(());

/// Runs `cmd` with sh; whether it succeeded. Logged either way.
fn sh(cmd: &str) -> bool {
    let out = Command::new("/bin/sh").arg("-c").arg(cmd).output();
    let ok = out.as_ref().is_ok_and(|o| o.status.success());
    let err = out.map(|o| String::from_utf8_lossy(&o.stderr).trim().to_string()).unwrap_or_else(|e| e.to_string());
    eprintln!(
        "net: {cmd} -> {}{}",
        if ok { "ok" } else { "FAILED" },
        if err.is_empty() { String::new() } else { format!(" ({err})") }
    );
    ok
}

/// The IPv4 address and netmask of `iface`, or of the first interface up that is
/// not loopback when `iface` is `None`.
pub fn ipv4(iface: Option<&str>) -> Option<(Ipv4Addr, Ipv4Addr)> {
    let mut found = None;
    unsafe {
        let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut list) != 0 {
            return None;
        }
        let mut cur = list;
        while !cur.is_null() {
            let ifa = &*cur;
            cur = ifa.ifa_next;
            if ifa.ifa_addr.is_null() || ifa.ifa_netmask.is_null() || (*ifa.ifa_addr).sa_family as i32 != libc::AF_INET
            {
                continue;
            }
            let name = std::ffi::CStr::from_ptr(ifa.ifa_name).to_string_lossy();
            let addr = Ipv4Addr::from(u32::from_be((*(ifa.ifa_addr as *const libc::sockaddr_in)).sin_addr.s_addr));
            let mask = Ipv4Addr::from(u32::from_be((*(ifa.ifa_netmask as *const libc::sockaddr_in)).sin_addr.s_addr));
            let wanted = match iface {
                Some(i) => name == i,
                None => !addr.is_loopback() && !addr.is_link_local(),
            };
            if wanted && !addr.is_unspecified() {
                found = Some((addr, mask));
                break;
            }
        }
        libc::freeifaddrs(list);
    }
    found
}

/// Wi-Fi on, then waits for an address; `cancel` gives up early.
pub fn join(cancel: &AtomicBool) -> Result<(Ipv4Addr, Ipv4Addr), String> {
    {
        let _net = NET.lock().unwrap();
        if cancel.load(Ordering::SeqCst) {
            return Err("Stopped".into());
        }
        if !sh("lipc-set-prop com.lab126.cmd wirelessEnable 1") {
            eprintln!("net: could not ask for Wi-Fi; waiting for an address anyway");
        }
    }
    let start = Instant::now();
    while start.elapsed() < JOIN_TIMEOUT {
        if cancel.load(Ordering::SeqCst) {
            return Err("Stopped".into());
        }
        if let Some(found) = ipv4(Some(IFACE)) {
            eprintln!("net: {IFACE} is {} mask {}", found.0, found.1);
            return Ok(found);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err("No Wi-Fi network joined".into())
}

/// Saves the rules in force, then shuts everything before the Wi-Fi comes on: only
/// loopback and DHCP (to get an address from the local network) pass, so nothing on
/// the Kindle reaches the Internet even for the seconds it takes to join. False, with
/// the rules left as they were, when they cannot be saved or applied, or `cancel` was
/// set meanwhile: the Wi-Fi then stays off.
pub fn lockdown(cancel: &AtomicBool) -> bool {
    let _net = NET.lock().unwrap();
    if cancel.load(Ordering::SeqCst) {
        return false;
    }
    // Saved once: a second round before teardown must not save our own rules as
    // "the rules before".
    if !std::path::Path::new(SAVED_V4).exists()
        && !sh(&format!("iptables-save > {SAVED_V4}.part && mv {SAVED_V4}.part {SAVED_V4}"))
    {
        eprintln!("net: cannot save the firewall: left as it is");
        let _ = std::fs::remove_file(format!("{SAVED_V4}.part"));
        return false;
    }
    let ok = sh(&format!(
        "iptables -P INPUT DROP && iptables -P OUTPUT DROP && iptables -P FORWARD DROP && \
         iptables -F && iptables -X && \
         iptables -A INPUT -i lo -j ACCEPT && iptables -A OUTPUT -o lo -j ACCEPT && \
         {DHCP}"
    ));
    if !ok {
        restore_v4();
        return false;
    }
    // IPv6 is not used at all. The Kindle has no ip6tables: IPv6 is switched off in
    // the kernel instead (where it has IPv6 at all), and back on at teardown.
    if !ipv6_off() {
        restore_v4();
        return false;
    }
    if sh(&format!("ip6tables-save > {SAVED_V6}.part && mv {SAVED_V6}.part {SAVED_V6}")) {
        sh("ip6tables -P INPUT DROP && ip6tables -P OUTPUT DROP && ip6tables -P FORWARD DROP && \
            ip6tables -F && ip6tables -A INPUT -i lo -j ACCEPT && ip6tables -A OUTPUT -o lo -j ACCEPT");
    } else {
        let _ = std::fs::remove_file(format!("{SAVED_V6}.part"));
    }
    true
}

const IPV6_CONF: &str = "/proc/sys/net/ipv6/conf";
/// The `disable_ipv6` values found before, one "name value" per line.
const SAVED_IPV6: &str = "/tmp/kindlewallet.ipv6";

/// IPv6 off for every interface (`all`, `default`, and wlan0 even before it exists
/// through `default`). True when done, or when the kernel has no IPv6.
fn ipv6_off() -> bool {
    let Ok(dir) = std::fs::read_dir(IPV6_CONF) else {
        eprintln!("net: no IPv6 in this kernel");
        return true;
    };
    if std::path::Path::new(SAVED_IPV6).exists() {
        // Already off from a round not torn down yet.
        return true;
    }
    let mut saved = String::new();
    for entry in dir.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().to_string();
        let file = entry.path().join("disable_ipv6");
        if let Ok(v) = std::fs::read_to_string(&file) {
            saved.push_str(&format!("{name} {}\n", v.trim()));
        }
    }
    if std::fs::write(SAVED_IPV6, &saved).is_err() {
        return false;
    }
    let mut ok = true;
    for line in saved.lines() {
        let name = line.split(' ').next().unwrap_or_default();
        ok &= std::fs::write(format!("{IPV6_CONF}/{name}/disable_ipv6"), "1").is_ok();
    }
    eprintln!("net: IPv6 off -> {}", if ok { "ok" } else { "FAILED" });
    ok
}

/// IPv6 as it was before [`ipv6_off`].
fn ipv6_restore() {
    let Ok(saved) = std::fs::read_to_string(SAVED_IPV6) else { return };
    for line in saved.lines() {
        if let Some((name, value)) = line.split_once(' ') {
            let _ = std::fs::write(format!("{IPV6_CONF}/{name}/disable_ipv6"), value);
        }
    }
    let _ = std::fs::remove_file(SAVED_IPV6);
    eprintln!("net: IPv6 restored");
}

/// DHCP only, on the Wi-Fi: the Kindle asking for an address and the answers.
const DHCP: &str = "iptables -A OUTPUT -o wlan0 -p udp --sport 68 --dport 67 -j ACCEPT && \
                    iptables -A INPUT -i wlan0 -p udp --sport 67 --dport 68 -j ACCEPT";

/// Once joined: the phone link and nothing else. In, TCP to `port` from the local
/// subnet; out, only that connection's replies (no DNS, nothing to the box or
/// beyond). On failure the lockdown stays in force.
pub fn firewall(ip: Ipv4Addr, mask: Ipv4Addr, port: u16, cancel: &AtomicBool) -> bool {
    let _net = NET.lock().unwrap();
    if cancel.load(Ordering::SeqCst) {
        return false;
    }
    let net = Ipv4Addr::from(u32::from(ip) & u32::from(mask));
    let subnet = format!("{net}/{}", u32::from(mask).count_ones());
    sh(&format!(
        "iptables -A INPUT -i {IFACE} -s {subnet} -p tcp --dport {port} -j ACCEPT && \
         iptables -A OUTPUT -o {IFACE} -d {subnet} -p tcp --sport {port} -j ACCEPT"
    ))
}

fn restore_v4() {
    if sh(&format!("iptables-restore < {SAVED_V4}")) {
        let _ = std::fs::remove_file(SAVED_V4);
    }
}

/// Proof the firewall holds: tries to reach the Internet (Cloudflare, Google, Amazon,
/// by address: there is no DNS) and reports whether anything got through. The link
/// starts only when nothing does.
pub fn internet_reachable() -> bool {
    use std::net::{SocketAddr, TcpStream, UdpSocket};
    let targets: [SocketAddr; 3] = [
        ([1, 1, 1, 1], 443).into(),
        ([8, 8, 8, 8], 53).into(),
        ([52, 94, 236, 248], 443).into(),
    ];
    let mut reached = false;
    for t in targets {
        if TcpStream::connect_timeout(&t, Duration::from_secs(3)).is_ok() {
            eprintln!("net: offline check: {t} REACHABLE");
            reached = true;
        }
    }
    // A DNS question out; the firewall refuses to send it.
    let udp = UdpSocket::bind("0.0.0.0:0").and_then(|u| u.send_to(&[0u8; 12], "8.8.8.8:53"));
    if udp.is_ok() {
        eprintln!("net: offline check: UDP to 8.8.8.8:53 left the Kindle");
        reached = true;
    }
    if !reached {
        eprintln!("net: offline check: no Internet (ok)");
    }
    reached
}

/// Puts the saved rules back and switches Wi-Fi off (safe to call any number of times).
pub fn teardown() {
    let _net = NET.lock().unwrap();
    for (saved, restore) in [(SAVED_V4, "iptables-restore"), (SAVED_V6, "ip6tables-restore")] {
        if std::path::Path::new(saved).exists() && sh(&format!("{restore} < {saved}")) {
            let _ = std::fs::remove_file(saved);
        }
    }
    sh("lipc-set-prop com.lab126.cmd wirelessEnable 0");
    ipv6_restore();
}
