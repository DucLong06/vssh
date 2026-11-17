//! Server metrics for the status line and the quick peek: the command that
//! collects them, the parser for its output, and each metric as a labelled
//! value with its level. How many of them fit on screen is the frontend's call.

use std::time::Duration;

use russh::client::Handle;

use crate::config::Lang;
use crate::ssh::{self, Client};

/// One round of metrics. Linux reads `/proc`; anything else (macOS) falls
/// back to `uptime`, `sysctl` and `vm_stat`. Wrapped in `sh -c` so the login
/// shell (fish, csh) does not have to understand it; its own lines are
/// labelled so the parser never has to guess which tool printed what.
macro_rules! script {
    ($more:literal) => {
        concat!(
            "sh -c 'exec 2>/dev/null; ",
            "if [ -r /proc/loadavg ]; then ",
            "echo load $(cat /proc/loadavg); ",
            "echo cpus $(grep -c ^processor /proc/cpuinfo); ",
            "grep MemTotal: /proc/meminfo; grep MemAvailable: /proc/meminfo; ",
            "else ",
            "echo load $(uptime); ",
            "echo cpus $(sysctl -n hw.ncpu); ",
            "echo membytes $(sysctl -n hw.memsize); ",
            "vm_stat; ",
            "fi; ",
            "df -Pk / | tail -1",
            $more,
            "'"
        )
    };
}
pub const COMMAND: &str = script!("");
/// The same round, plus what a person looks at once: uptime, system, name.
pub const PEEK: &str =
    script!("; echo uptime $(uptime); echo uname $(uname -srm); echo host $(hostname)");

/// How often a session asks.
const EVERY: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Stats {
    /// 1-minute load average per core, in per cent. Over 100 means a queue.
    pub cpu: Option<u32>,
    /// Memory in use and in total, in KiB.
    pub mem: Option<(u64, u64)>,
    /// How full `/` is, in per cent.
    pub disk: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Level {
    Ok,
    Warn,
    Crit,
}

fn level(pct: u32, warn: u32, crit: u32) -> Level {
    match pct {
        p if p > crit => Level::Crit,
        p if p > warn => Level::Warn,
        _ => Level::Ok,
    }
}

/// The load averages of a `load` line. `/proc/loadavg` starts with them;
/// `uptime` has them after "load average:" or "load averages:", with commas
/// between them and, in some locales, a comma as the decimal sign.
fn loads(v: &str) -> Vec<f64> {
    let v = v.rsplit_once("load average").map_or(v, |(_, after)| after);
    let words = v.trim_start_matches(['s', ':']).split_whitespace().take(3);
    words
        .map_while(|w| w.trim_end_matches(',').replace(',', ".").parse().ok())
        .collect()
}

/// What the quick peek shows: the metrics, and the lines only [`PEEK`] asks
/// for. Anything the host did not answer stays empty.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Peek {
    pub stats: Stats,
    /// The 1, 5 and 15 minute load averages.
    pub load: Option<String>,
    /// How long the machine has been up, in `uptime`'s words.
    pub uptime: Option<String>,
    /// `uname -srm`.
    pub os: Option<String>,
    pub host: Option<String>,
}

/// [`Peek::uptime`] in this language: `uptime` itself speaks English only.
/// `41 days, 3 min` is `41 ngày, 3 phút`; the `2:15` form needs nothing.
pub fn uptime(up: &str, lang: Lang) -> String {
    let word = |w: &str| {
        let unit = w.trim_end_matches(',');
        let vi = match unit.trim_end_matches('s') {
            "day" => "ngày",
            "hr" => "giờ",
            "min" => "phút",
            "sec" => "giây",
            _ => return w.to_string(),
        };
        format!("{vi}{}", &w[unit.len()..])
    };
    match lang {
        Lang::En => up.to_string(),
        Lang::Vi => up.split(' ').map(word).collect::<Vec<_>>().join(" "),
    }
}

/// Parses the output of [`PEEK`]. Never fails: what parsed is filled in.
pub fn parse_peek(out: &str) -> Peek {
    let mut peek = Peek {
        stats: parse(out).unwrap_or_default(),
        ..Peek::default()
    };
    let text = |v: &str| Some(v.trim().to_string()).filter(|v| !v.is_empty());
    for line in out.lines().map(str::trim) {
        if let Some(v) = line.strip_prefix("load ") {
            let all: Vec<String> = loads(v).iter().map(|l| format!("{l:.2}")).collect();
            peek.load = text(&all.join(" "));
        } else if let Some(v) = line.strip_prefix("uptime ") {
            // " 10:15 up 3 days, 2:15, 2 users, load average: …": the part
            // between "up" and the users or the load.
            let up = v.split_once("up ").map_or("", |(_, after)| after);
            let parts = up.split(',').map(str::trim);
            let parts: Vec<&str> = parts
                .take_while(|p| !p.contains("user") && !p.contains("load"))
                .collect();
            peek.uptime = text(&parts.join(", "));
        } else if let Some(v) = line.strip_prefix("uname ") {
            peek.os = text(v);
        } else if let Some(v) = line.strip_prefix("host ") {
            peek.host = text(v);
        }
    }
    peek
}

/// Parses the output of [`COMMAND`]. Lines it does not know are skipped, so
/// a login banner or a missing tool costs one metric, not all of them.
/// `None` if nothing usable came back.
pub fn parse(out: &str) -> Option<Stats> {
    // vm_stat ends its numbers with a dot.
    let num = |s: &str| {
        let word = s.split_whitespace().next()?;
        word.trim_end_matches('.').parse::<u64>().ok()
    };
    let (mut load, mut cpus, mut total, mut free) = (None, None, None, None);
    // macOS only: the page size, and the pages nothing holds on to.
    let (mut page, mut idle) = (4096, None::<u64>);
    let mut stats = Stats::default();
    for line in out.lines().map(str::trim) {
        if let Some(v) = line.strip_prefix("load ") {
            load = loads(v).first().copied();
        } else if let Some(v) = line.strip_prefix("cpus ") {
            cpus = num(v);
        } else if let Some(v) = line.strip_prefix("membytes ") {
            total = num(v).map(|bytes| bytes / 1024);
        } else if let Some(v) = line.strip_prefix("MemTotal:") {
            total = num(v);
        } else if let Some(v) = line.strip_prefix("MemAvailable:") {
            free = num(v);
        } else if let Some((_, v)) = line.split_once("page size of ") {
            page = num(v).unwrap_or(page);
        } else if let Some(v) = ["Pages free:", "Pages inactive:", "Pages speculative:"]
            .iter()
            .find_map(|key| line.strip_prefix(key))
        {
            idle = Some(idle.unwrap_or(0) + num(v).unwrap_or(0));
        } else if line.ends_with(" /") {
            // The `df -P` row of the root file system: its capacity is the `61%` column.
            let pct = line.split_whitespace().find_map(|w| w.strip_suffix('%'));
            stats.disk = pct.and_then(|p| p.parse().ok());
        }
    }
    let free = free.or(idle.map(|pages| pages * page / 1024));
    stats.cpu = match (load, cpus) {
        (Some(load), Some(cpus)) if cpus > 0 => Some((load * 100.0 / cpus as f64).round() as u32),
        _ => None,
    };
    stats.mem = total
        .zip(free)
        .filter(|(total, _)| *total > 0)
        .map(|(total, free)| (total.saturating_sub(free), total));
    (stats != Stats::default()).then_some(stats)
}

/// KiB as GiB.
fn gib(kib: u64) -> f64 {
    kib as f64 / (1024.0 * 1024.0)
}

/// One cell that fills up with the percentage.
fn bar(pct: u32) -> char {
    const STEPS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    STEPS[(pct.min(100) * 7 / 100) as usize]
}

/// Cells a row of segments takes, with ` · ` between them.
pub fn width(parts: &[(String, String, Level)]) -> usize {
    let cells = |(label, value, _): &(String, String, Level)| {
        label.chars().count() + 1 + value.chars().count()
    };
    parts.iter().map(cells).sum::<usize>() + 3 * parts.len().saturating_sub(1)
}

impl Stats {
    /// Label, value and level of every metric there is, most important first.
    pub fn parts(&self, mem_label: &str, bars: bool) -> Vec<(String, String, Level)> {
        let gauge = |pct: u32| match bars {
            true => format!("{} ", bar(pct)),
            false => String::new(),
        };
        let mut parts = vec![];
        if let Some(cpu) = self.cpu {
            let value = format!("{}{}%", gauge(cpu), cpu.min(999));
            parts.push(("CPU".to_string(), value, level(cpu, 50, 80)));
        }
        if let Some((used, total)) = self.mem {
            let pct = (used * 100 / total.max(1)) as u32;
            // A small machine keeps the decimal: 0.5G, not 0G or 1G.
            let decimals = usize::from(gib(total) < 10.0);
            let value = format!("{}{:.1}/{:.decimals$}G", gauge(pct), gib(used), gib(total));
            parts.push((mem_label.to_string(), value, level(pct, 70, 85)));
        }
        if let Some(disk) = self.disk {
            let value = format!("{}{disk}%", gauge(disk));
            parts.push(("/".to_string(), value, level(disk, 80, 90)));
        }
        parts
    }
}

/// One round over an open connection: a second channel, not a second login.
/// `None` if the host does not answer in time or says nothing usable.
pub async fn fetch(h: &Handle<Client>) -> Option<Stats> {
    let out = tokio::time::timeout(Duration::from_secs(4), ssh::exec(h, COMMAND)).await;
    parse(&out.ok()?.ok()?)
}

/// Reports a session's metrics every few seconds for as long as it is polled.
/// It never returns: after three failures in a row (a server that allows one
/// channel per connection, a forced command) it stops asking and only waits.
pub async fn poll(h: &Handle<Client>, report: impl Fn(Option<Stats>)) {
    let mut failures = 0;
    while failures < 3 {
        let stats = fetch(h).await;
        failures = if stats.is_some() { 0 } else { failures + 1 };
        report(stats);
        tokio::time::sleep(EVERY).await;
    }
    std::future::pending().await
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINUX: &str = "load 0.92 0.58 0.59 1/389 12345\n\
        cpus 4\n\
        MemTotal:       16303428 kB\n\
        MemAvailable:   11883116 kB\n\
        /dev/nvme0n1p2   487652000 278412000 184340000      61% /\n";

    const MACOS: &str = "load 9:41 up 3 days, 2:15, 2 users, load averages: 6.80 1.60 1.71\n\
        cpus 8\n\
        membytes 17179869184\n\
        Mach Virtual Memory Statistics: (page size of 16384 bytes)\n\
        Pages free:                               65536.\n\
        Pages active:                            300000.\n\
        Pages inactive:                          131072.\n\
        Pages speculative:                        65536.\n\
        Pages wired down:                        120000.\n\
        /dev/disk3s1s1 965595304 9876543 512345678    92% /\n";

    #[test]
    fn parses_proc_meminfo_and_df() {
        let stats = parse(LINUX).unwrap();
        // Load 0.92 on 4 cores; 16303428 - 11883116 kB in use.
        assert_eq!(stats.cpu, Some(23));
        assert_eq!(stats.mem, Some((4_420_312, 16_303_428)));
        assert_eq!(stats.disk, Some(61));
    }

    #[test]
    fn parses_the_macos_fallback() {
        let stats = parse(MACOS).unwrap();
        assert_eq!(stats.cpu, Some(85));
        // 16 GiB, of which 262144 pages of 16 KiB (4 GiB) are free, inactive or speculative.
        assert_eq!(stats.mem, Some((12 * 1024 * 1024, 16 * 1024 * 1024)));
        assert_eq!(stats.disk, Some(92));
    }

    #[test]
    fn parses_what_is_there_and_nothing_from_noise() {
        // Linux `uptime` with a decimal comma, no memory lines, a banner, a failed exit.
        let partial = "Welcome to web-01\n\
            load 10:15:01 up 3 days, 2 users, load average: 1,50, 0,58, 0,59\n\
            cpus 2\n\
            [exit status 1]\n";
        let stats = parse(partial).unwrap();
        assert_eq!((stats.cpu, stats.mem, stats.disk), (Some(75), None, None));

        // No core count: a load figure alone is not a percentage.
        assert_eq!(parse("load 0.50 0.40 0.30 1/100 99\ncpus\n"), None);
        assert_eq!(parse("load 0.50 0.40 0.30 1/100 99\ncpus 0\n"), None);
        // A kernel without MemAvailable: no memory figure rather than a wrong one.
        assert_eq!(parse("MemTotal: 1000 kB\n"), None);
        for noise in ["", "\n\n", "This account is restricted.\n", "load\ncpus\n"] {
            assert_eq!(parse(noise), None, "{noise:?}");
        }
    }

    #[test]
    fn peek_adds_uptime_system_and_name() {
        let linux = format!(
            "{LINUX}uptime 10:15:01 up 3 days, 2:15, 2 users, load average: 0.92, 0.58, 0.59\n\
             uname Linux 6.14.0-37-generic x86_64\nhost web-01\n"
        );
        let peek = parse_peek(&linux);
        assert_eq!(peek.stats, parse(LINUX).unwrap());
        assert_eq!(peek.load.as_deref(), Some("0.92 0.58 0.59"));
        assert_eq!(peek.uptime.as_deref(), Some("3 days, 2:15"));
        assert_eq!(peek.os.as_deref(), Some("Linux 6.14.0-37-generic x86_64"));
        assert_eq!(peek.host.as_deref(), Some("web-01"));

        // macOS: the load comes from `uptime`.
        let mac = format!("{MACOS}uname Darwin 24.1.0 arm64\n");
        let peek = parse_peek(&mac);
        assert_eq!(peek.load.as_deref(), Some("6.80 1.60 1.71"));
        assert_eq!(peek.os.as_deref(), Some("Darwin 24.1.0 arm64"));
        assert_eq!(peek.host, None);
        // busybox has no users in it; a young machine has minutes only; a decimal comma.
        let up = |line: &str| parse_peek(line).uptime;
        let young = "uptime 08:00:01 up 12 min, load average: 0.00, 0.01, 0.05";
        assert_eq!(up(young).as_deref(), Some("12 min"));
        let old = "uptime 08:00:01 up 41 days, 3 min, 1 user, load average: 0,10, 0,20, 0,30";
        assert_eq!(up(old).as_deref(), Some("41 days, 3 min"));

        // In Vietnamese the units are translated: Linux's, and macOS's hrs, mins and secs.
        let vi = |up: &str| uptime(up, Lang::Vi);
        assert_eq!(vi("41 days, 3 min"), "41 ngày, 3 phút");
        assert_eq!(vi("1 day, 2:15"), "1 ngày, 2:15");
        assert_eq!(vi("2 hrs"), "2 giờ");
        assert_eq!(vi("5 mins"), "5 phút");
        assert_eq!(vi("30 secs"), "30 giây");
        assert_eq!(vi("2:15"), "2:15");
        assert_eq!(uptime("41 days, 3 min", Lang::En), "41 days, 3 min");

        // What did not come back stays empty, and what did is kept.
        let partial = parse_peek("uname Linux 5.10 aarch64\nuptime\nhost\n[exit status 127]\n");
        assert_eq!(partial.os.as_deref(), Some("Linux 5.10 aarch64"));
        assert_eq!(
            (partial.uptime, partial.host, partial.load),
            (None, None, None)
        );
        assert_eq!(partial.stats, Stats::default());
        assert_eq!(parse_peek(""), Peek::default());
    }

    fn text(parts: &[(String, String, Level)]) -> String {
        let joined: Vec<String> = parts.iter().map(|(l, v, _)| format!("{l} {v}")).collect();
        joined.join(" · ")
    }

    #[test]
    fn levels_and_small_machines() {
        // cpu warns over 50 and is critical over 80; memory over 70 and 85.
        assert_eq!(
            [50, 51, 80, 81].map(|p| level(p, 50, 80)),
            [Level::Ok, Level::Warn, Level::Warn, Level::Crit]
        );
        let stats = Stats {
            cpu: Some(250),
            mem: Some((460_000, 512_000)),
            disk: Some(85),
        };
        let parts = stats.parts("MEM", false);
        assert_eq!(text(&parts), "CPU 250% · MEM 0.4/0.5G · / 85%");
        let levels: Vec<Level> = parts.iter().map(|p| p.2).collect();
        assert_eq!(levels, [Level::Crit, Level::Crit, Level::Warn]);
        assert_eq!((bar(0), bar(50), bar(100), bar(250)), ('▁', '▄', '█', '█'));
    }
}
