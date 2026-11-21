//! A jump host that was given a key of its own (`Target::with_jump_auth`),
//! against a real sshd on loopback: this user's, on a port of its own, under
//! a throwaway HOME, killed at the end. It plays the jump host and the host
//! behind it, and opens to one key only. Without `/usr/sbin/sshd` the test
//! says so and proves nothing.
#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use vssh_core::conn_err::Fail;
use vssh_core::ssh::{self, generate_key, HopAuth, JumpAuth, Target};
use vssh_core::store::ssh_config::SshConfig;

const SSHD: &str = "/usr/sbin/sshd";

struct Sshd(Child);

impl Drop for Sshd {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// An sshd that lets this user in with the keys of `home/authorized_keys`.
fn sshd(home: &Path, port: u16) -> Sshd {
    let at = |name: &str| home.join(name).display().to_string();
    generate_key(&home.join("host_key"), "host").unwrap();
    let config = format!(
        "ListenAddress 127.0.0.1:{port}\nHostKey {}\nPidFile {}\nAuthorizedKeysFile {}\n\
         StrictModes no\nUsePAM no\nPasswordAuthentication no\nKbdInteractiveAuthentication no\n",
        at("host_key"),
        at("pid"),
        at("authorized_keys"),
    );
    fs::write(home.join("sshd_config"), config).unwrap();
    let log = fs::File::create(home.join("sshd.log")).unwrap();
    let child = Command::new(SSHD)
        .args(["-D", "-e", "-f", &at("sshd_config")])
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .unwrap();
    Sshd(child)
}

#[test]
fn the_key_a_hop_was_given_is_the_one_it_is_logged_in_to_with() {
    if !Path::new(SSHD).exists() {
        let _ = writeln!(std::io::stderr(), "SKIPPED: no {SSHD} on this machine");
        return;
    }
    let home = std::env::temp_dir().join(format!("vssh-jump-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(home.join(".ssh")).unwrap();
    // The core keeps known_hosts under $HOME, and would offer the agent's keys.
    std::env::set_var("HOME", &home);
    std::env::remove_var("SSH_AUTH_SOCK");

    // Two keys; the sshd knows the second only.
    generate_key(&home.join("refused"), "refused").unwrap();
    let opens = generate_key(&home.join("opens"), "opens").unwrap();
    fs::write(home.join("authorized_keys"), opens).unwrap();
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let _sshd = sshd(&home, port);

    let key = |name: &str| home.join(name).display().to_string();
    let config = |bastion_key: &str| {
        SshConfig::parse(&format!(
            "Host bastion\n  HostName 127.0.0.1\n  Port {port}\n  IdentityFile {}\n\
             Host inner\n  HostName 127.0.0.1\n  Port {port}\n  IdentityFile {}\n  ProxyJump bastion\n",
            key(bastion_key),
            key("opens"),
        ))
    };
    let given = |name: &str| {
        let auth = HopAuth {
            key: key(name),
            user: String::new(),
        };
        JumpAuth::from([(("inner".to_string(), "bastion".to_string()), auth)])
    };
    // Why the road to `inner` ended at its jump host, if it did.
    let hop_refused = |config: &SshConfig, auth: &JumpAuth| {
        let target = Target::with_jump_auth(config, "inner", auth).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            // The sshd may still be on its way up.
            for _ in 0..50 {
                let up = tokio::net::TcpStream::connect(("127.0.0.1", port)).await;
                if up.is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            match ssh::connect(&target, None, None).await {
                Ok(h) => {
                    // All the way: the host behind the jump host answers.
                    let said = ssh::exec(&h, "echo through").await.unwrap();
                    assert_eq!(said.trim(), "through");
                    None
                }
                Err(e) => Some(e.downcast_ref::<Fail>().cloned().expect("a Fail")),
            }
        })
    };
    let at_the_hop = |fail: Option<Fail>| match fail {
        Some(Fail::Jump { hop, cause }) => {
            assert_eq!(hop, format!("127.0.0.1:{port} (hop 1 of 1)"));
            let keys = match cause.as_deref() {
                Some(Fail::Auth { keys, .. }) => *keys,
                other => panic!("the hop failed for another reason: {other:?}"),
            };
            assert_eq!(keys, 1, "one key was offered: the one that was given");
        }
        other => panic!("expected the jump host to refuse: {other:?}"),
    };

    let nothing = JumpAuth::new();
    // The jump host's block names a key the sshd refuses: no way through...
    at_the_hop(hop_refused(&config("refused"), &nothing));
    // ...until the hop is given the key that opens it. The block is as it was.
    assert_eq!(hop_refused(&config("refused"), &given("opens")), None);
    // And the other way round: the block's key would open it, the key that
    // was given does not, and that is the one that counts.
    assert_eq!(hop_refused(&config("opens"), &nothing), None);
    at_the_hop(hop_refused(&config("opens"), &given("refused")));

    drop(_sshd);
    fs::remove_dir_all(&home).unwrap();
}
