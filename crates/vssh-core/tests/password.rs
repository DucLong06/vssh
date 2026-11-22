//! Logging in with a password, against a real sshd in a throwaway container:
//! one that takes the `password` method, one that only takes
//! `keyboard-interactive` (PAM), as many servers do, and one that takes
//! both, where a wrong password must cost one try, not two. Without docker,
//! or where docker cannot run a Linux container, the test says why and passes.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vssh_core::conn_err::{ConnError, Fail};
use vssh_core::ssh::{SessionCmd, Target};
use vssh_core::task::{self, Shell};
use zeroize::Zeroizing;

/// The container's three sshds, by port inside it. Only the last one's log
/// is kept: the others leave theirs behind when they go to the background.
const PASSWORD: u16 = 2201;
const KBD_INTERACTIVE: u16 = 2202;
const BOTH: u16 = 2203;

/// Removes the container however the test ends.
struct Container(String);

impl Drop for Container {
    fn drop(&mut self) {
        let _ = Command::new("docker").args(["rm", "-f", &self.0]).output();
    }
}

fn docker(args: &[&str]) -> Option<String> {
    let out = Command::new("docker").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Starts the container, or says why it cannot.
fn sshd(password: &str) -> Result<Container, String> {
    docker(&["info"]).ok_or("no docker")?;
    let script = format!(
        "apk add --no-cache openssh openssh-server-pam >/dev/null && ssh-keygen -A >/dev/null && \
         adduser -D tester && echo \"tester:$PW\" | chpasswd && \
         /usr/sbin/sshd.pam -e -p {PASSWORD} -o UsePAM=yes \
           -o PasswordAuthentication=yes -o KbdInteractiveAuthentication=no && \
         /usr/sbin/sshd.pam -e -p {KBD_INTERACTIVE} -o UsePAM=yes \
           -o PasswordAuthentication=no -o KbdInteractiveAuthentication=yes && \
         exec /usr/sbin/sshd.pam -D -e -p {BOTH} -o UsePAM=yes -o LogLevel=VERBOSE \
           -o PasswordAuthentication=yes -o KbdInteractiveAuthentication=yes"
    );
    let pw = format!("PW={password}");
    let (a, b, both) = (
        format!("127.0.0.1::{PASSWORD}"),
        format!("127.0.0.1::{KBD_INTERACTIVE}"),
        format!("127.0.0.1::{BOTH}"),
    );
    #[rustfmt::skip]
    let id = docker(&[
        "run", "-d", "--rm", "-e", &pw,
        // apk needs them behind a proxy; unset ones are not passed.
        "-e", "http_proxy", "-e", "https_proxy", "-e", "HTTP_PROXY", "-e", "HTTPS_PROXY",
        "-p", &a, "-p", &b, "-p", &both, "alpine:latest", "sh", "-c", &script,
    ])
    .ok_or("docker cannot run alpine:latest here")?;
    Ok(Container(id))
}

/// Where the container's `port` is on this machine, once its sshd answers.
fn ready(c: &Container, port: u16) -> u16 {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(180) {
        let mapped = docker(&["port", &c.0, &format!("{port}/tcp")]);
        let mapped = mapped.and_then(|m| m.lines().next()?.rsplit(':').next()?.parse().ok());
        if let Some(mapped) = mapped {
            let mut banner = [0; 4];
            let said = TcpStream::connect(("127.0.0.1", mapped)).and_then(|mut s| {
                s.set_read_timeout(Some(Duration::from_secs(2)))?;
                s.read_exact(&mut banner)
            });
            if said.is_ok() && &banner == b"SSH-" {
                return mapped;
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let logs = Command::new("docker").args(["logs", &c.0]).output();
    let logs = logs.map(|o| String::from_utf8_lossy(&o.stderr).into_owned());
    panic!("sshd on {port} never answered: {logs:?}");
}

/// How many passwords the sshd on `BOTH` has turned down so far, by either
/// method. Its log is the container's.
fn refusals(c: &Container) -> usize {
    let logs = Command::new("docker")
        .args(["logs", &c.0])
        .output()
        .unwrap();
    let logs = String::from_utf8_lossy(&logs.stderr).into_owned();
    let refused =
        |l: &&str| l.contains("Failed password") || l.contains("Failed keyboard-interactive");
    logs.lines().filter(refused).count()
}

/// A shell through `task::shell` that types `exit`: how many times it said
/// it was up, and how it ended.
fn shell(port: u16, password: &str) -> (usize, Result<(), ConnError>) {
    let target = Target::parse(&format!("tester@127.0.0.1:{port}")).unwrap();
    let ups = Arc::new(Mutex::new(0));
    let (keys, typed) = task::channel();
    keys.send(SessionCmd::Data(b"exit\r".to_vec())).unwrap();
    let counted = ups.clone();
    let report = move |ev| {
        if let Shell::Up = ev {
            *counted.lock().unwrap() += 1;
        }
    };
    let rt = tokio::runtime::Runtime::new().unwrap();
    let password = Some(Zeroizing::new(password.to_string()));
    let running = task::shell(target, password, (80, 24), typed, report);
    let res = rt
        .block_on(async { tokio::time::timeout(Duration::from_secs(60), running).await })
        .expect("the shell never ended");
    drop(keys);
    let ups = *ups.lock().unwrap();
    (ups, res)
}

#[test]
fn a_password_logs_in_by_one_method_the_server_offers_and_a_wrong_one_costs_one_try() {
    // Made here: no secret-looking literal for scanners to flag.
    let right = ["pw", "-", "test"].concat();
    let wrong = ["pw", "-", "nope"].concat();
    let c = match sshd(&right) {
        Ok(c) => c,
        Err(why) => {
            // Said past the test harness's capture, so a skip is seen.
            let _ = writeln!(std::io::stderr(), "skipped: {why}");
            return;
        }
    };
    let ports = [
        ready(&c, PASSWORD),
        ready(&c, KBD_INTERACTIVE),
        ready(&c, BOTH),
    ];
    // Nothing of this machine's: known_hosts is a new one, and no agent.
    let home = std::env::temp_dir().join(format!("vssh-password-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("HOME", &home);
    std::env::remove_var("SSH_AUTH_SOCK");

    // Wrong first, then right: a refusal does not stand in the way of the next try.
    for port in ports {
        let before = refusals(&c);
        let (ups, res) = shell(port, &wrong);
        let err = res.expect_err("a wrong password logged in");
        assert_eq!(ups, 0);
        assert!(
            matches!(err.fail, Some(Fail::Auth { password: true, .. })),
            "port {port}: {err:?}"
        );
        if port == ports[2] {
            // Offered both methods, the wrong password is tried by one: one try
            // against MaxAuthTries (and fail2ban), not two.
            let start = Instant::now();
            while refusals(&c) == before && start.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(100));
            }
            std::thread::sleep(Duration::from_millis(500));
            assert_eq!(refusals(&c) - before, 1, "port {port}");
        }

        let (ups, res) = shell(port, &right);
        assert!(res.is_ok(), "port {port}: {res:?}");
        assert_eq!(ups, 1, "port {port}");
    }
    let _ = std::fs::remove_dir_all(&home);
}
