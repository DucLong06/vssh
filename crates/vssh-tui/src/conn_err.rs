//! Why a connection failed. The connect path tags an error with what it knew
//! at that point (`Fail`); `classify` turns any error chain into a class and
//! a message a person can act on. An error it does not know is shown as it
//! is, never guessed at.

use std::fmt;
use std::io::ErrorKind;
use std::path::PathBuf;

use vssh_core::config::Lang;

/// What the connect path knew when it gave up. Lives in the error chain
/// (as the error itself or as its context) and is found there by `classify`.
#[derive(Clone, Debug, PartialEq)]
pub enum Fail {
    Dns {
        host: String,
    },
    /// `addr` is `ip:port` everywhere below.
    Timeout {
        addr: String,
        secs: u64,
    },
    Refused {
        addr: String,
    },
    Unreachable {
        addr: String,
    },
    /// Every method was refused: how many keys were offered, and whether a
    /// password was. The caller may retry with a password.
    Auth {
        user: String,
        host: String,
        keys: usize,
        password: bool,
    },
    HostKey {
        host: String,
        /// Of the key the server presented.
        fingerprint: String,
        /// Of the conflicting entry in `file`.
        line: usize,
        file: PathBuf,
    },
    /// A hop of a `ProxyJump` chain failed; `cause` is why, if it is known.
    Jump {
        hop: String,
        cause: Option<Box<Fail>>,
    },
}

/// English, for the raw chain; `Fail::message` is what people read.
impl fmt::Display for Fail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dns { host } => write!(f, "cannot resolve {host}"),
            Self::Timeout { addr, secs } => write!(f, "{addr} did not answer within {secs}s"),
            Self::Refused { addr } => write!(f, "connection refused by {addr}"),
            Self::Unreachable { addr } => write!(f, "cannot reach {addr}"),
            Self::Auth { user, host, .. } => write!(f, "authentication failed for {user}@{host}"),
            Self::HostKey {
                host,
                fingerprint,
                line,
                file,
            } => write!(
                f,
                "HOST KEY MISMATCH for {host}: the server's key ({fingerprint}) is not the one \
                 on line {line} of {}. Refusing to connect; fix that file if the key really changed",
                file.display()
            ),
            Self::Jump { hop, .. } => write!(f, "jump host {hop} failed"),
        }
    }
}

impl std::error::Error for Fail {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Class {
    Dns,
    Timeout,
    Refused,
    Unreachable,
    Auth,
    HostKey,
    Jump,
    /// Not one of the above: shown verbatim.
    Other,
}

impl Class {
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::Dns => "DNS",
            Self::Timeout => lang.pick("TIMEOUT", "HẾT GIỜ"),
            Self::Refused => lang.pick("REFUSED", "BỊ TỪ CHỐI"),
            Self::Unreachable => lang.pick("UNREACHABLE", "KHÔNG TỚI ĐƯỢC"),
            Self::Auth => lang.pick("AUTH", "XÁC THỰC"),
            Self::HostKey => lang.pick("HOST KEY", "KHOÁ MÁY CHỦ"),
            Self::Jump => lang.pick("JUMP HOST", "MÁY TRUNG GIAN"),
            Self::Other => lang.pick("ERROR", "LỖI"),
        }
    }
}

impl Fail {
    pub fn class(&self) -> Class {
        match self {
            Self::Dns { .. } => Class::Dns,
            Self::Timeout { .. } => Class::Timeout,
            Self::Refused { .. } => Class::Refused,
            Self::Unreachable { .. } => Class::Unreachable,
            Self::Auth { .. } => Class::Auth,
            Self::HostKey { .. } => Class::HostKey,
            Self::Jump { .. } => Class::Jump,
        }
    }

    /// The message for people. `under` is the chain below this failure: the
    /// operating system's or the library's own words.
    fn message(&self, lang: Lang, under: &[String]) -> String {
        let t = |en: String, vi: String| if lang == Lang::Vi { vi } else { en };
        let below = under.join(": ");
        match self {
            Self::Dns { host } => t(
                format!("Cannot resolve {host}: {below}"),
                format!("Không phân giải được tên {host}: {below}"),
            ),
            Self::Timeout { addr, secs } => t(
                format!("Connection timed out after {secs}s ({addr})"),
                format!("Hết thời gian kết nối sau {secs} giây ({addr})"),
            ),
            Self::Refused { addr } => t(
                format!("Connection refused by {addr} — is sshd running?"),
                format!("{addr} từ chối kết nối — sshd có đang chạy không?"),
            ),
            Self::Unreachable { addr } => t(
                format!("Cannot reach {addr}: {below}"),
                format!("Không tới được {addr}: {below}"),
            ),
            Self::Auth {
                user,
                host,
                password: true,
                ..
            } => t(
                format!("Wrong password for {user}@{host}"),
                format!("Sai mật khẩu cho {user}@{host}"),
            ),
            Self::Auth {
                user, host, keys, ..
            } if *keys > 0 => t(
                format!("Public key rejected for {user}@{host}: {keys} offered, none accepted"),
                format!("Khoá công khai bị từ chối cho {user}@{host}: đã thử {keys} khoá, không khoá nào được nhận"),
            ),
            Self::Auth { user, host, .. } => t(
                format!("No way to log in as {user}@{host}: no key in the agent or the key files, and no password"),
                format!("Không có cách đăng nhập {user}@{host}: không có khoá trong agent hay file khoá, và không có mật khẩu"),
            ),
            Self::HostKey {
                host,
                fingerprint,
                line,
                file,
            } => t(
                format!(
                    "Host key mismatch for {host}: the server sent {fingerprint}, but line {line} of {} holds another key",
                    file.display()
                ),
                format!(
                    "Khoá máy chủ của {host} không khớp: máy chủ gửi {fingerprint}, nhưng dòng {line} của {} là khoá khác",
                    file.display()
                ),
            ),
            Self::Jump { hop, cause } => {
                let why = match cause {
                    // The cause is the next entry of the chain: its own words lie under it.
                    Some(cause) => cause.message(lang, under.get(1..).unwrap_or_default()),
                    None => below,
                };
                t(
                    format!("Jump host {hop} failed: {why}"),
                    format!("Máy trung gian {hop} lỗi: {why}"),
                )
            }
        }
    }
}

/// A classified failure, ready to show.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub class: Class,
    pub message: String,
    /// Every error in the chain, outermost first, joined by `: `.
    pub chain: String,
}

/// The class of an error that carries no `Fail`, from the types the
/// libraries use. `None` for anything not listed here.
fn untagged(err: &anyhow::Error) -> Option<Class> {
    let io = |kind: ErrorKind| match kind {
        ErrorKind::ConnectionRefused => Some(Class::Refused),
        ErrorKind::TimedOut => Some(Class::Timeout),
        ErrorKind::HostUnreachable | ErrorKind::NetworkUnreachable => Some(Class::Unreachable),
        _ => None,
    };
    err.chain().find_map(|e| {
        if let Some(e) = e.downcast_ref::<std::io::Error>() {
            return io(e.kind());
        }
        if e.downcast_ref::<tokio::time::error::Elapsed>().is_some() {
            return Some(Class::Timeout);
        }
        match e.downcast_ref::<russh::Error>()? {
            russh::Error::IO(e) => io(e.kind()),
            russh::Error::ConnectionTimeout
            | russh::Error::KeepaliveTimeout
            | russh::Error::InactivityTimeout
            | russh::Error::Elapsed(_) => Some(Class::Timeout),
            russh::Error::NoAuthMethod
            | russh::Error::NotAuthenticated
            | russh::Error::UnsupportedAuthMethod => Some(Class::Auth),
            russh::Error::UnknownKey
            | russh::Error::WrongServerSig
            | russh::Error::KeyChanged { .. } => Some(Class::HostKey),
            _ => None,
        }
    })
}

/// Turns an error chain into what to show. A `Fail` in it gives a tailored
/// message; a library error of a known kind gives the class with the chain
/// verbatim; anything else is `Class::Other` with the chain verbatim.
pub fn classify(err: &anyhow::Error, lang: Lang) -> Report {
    let chain = format!("{err:#}");
    let below: Vec<String> = err.chain().skip(1).map(ToString::to_string).collect();
    let (class, message) = match err.downcast_ref::<Fail>() {
        Some(fail) => (fail.class(), fail.message(lang, &below)),
        None => (untagged(err).unwrap_or(Class::Other), chain.clone()),
    };
    Report {
        class,
        message,
        chain,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use std::io;

    fn en(err: &anyhow::Error) -> (Class, String) {
        let report = classify(err, Lang::En);
        (report.class, report.message)
    }

    fn os(kind: ErrorKind, text: &str) -> anyhow::Error {
        anyhow::Error::new(io::Error::new(kind, text.to_string()))
    }

    #[test]
    fn dns_timeout_refused_unreachable() {
        let lookup = "failed to lookup address information: Name or service not known";
        let err = os(ErrorKind::Other, lookup).context(Fail::Dns {
            host: "db.nope.invalid".into(),
        });
        let dns = format!("Cannot resolve db.nope.invalid: {lookup}");
        assert_eq!(en(&err), (Class::Dns, dns));
        assert_eq!(
            classify(&err, Lang::En).chain,
            format!("cannot resolve db.nope.invalid: {lookup}")
        );

        let addr = "203.0.113.7:22".to_string();
        let err = anyhow::Error::new(Fail::Timeout {
            addr: addr.clone(),
            secs: 10,
        });
        let timeout = "Connection timed out after 10s (203.0.113.7:22)";
        assert_eq!(en(&err), (Class::Timeout, timeout.into()));

        let err = os(
            ErrorKind::ConnectionRefused,
            "Connection refused (os error 111)",
        )
        .context(Fail::Refused { addr: addr.clone() });
        let refused = "Connection refused by 203.0.113.7:22 — is sshd running?";
        assert_eq!(en(&err), (Class::Refused, refused.into()));
        assert_eq!(
            classify(&err, Lang::Vi).message,
            "203.0.113.7:22 từ chối kết nối — sshd có đang chạy không?"
        );
        assert_eq!(classify(&err, Lang::Vi).class.label(Lang::Vi), "BỊ TỪ CHỐI");

        let err = os(
            ErrorKind::NetworkUnreachable,
            "Network is unreachable (os error 101)",
        )
        .context(Fail::Unreachable { addr });
        let unreachable = "Cannot reach 203.0.113.7:22: Network is unreachable (os error 101)";
        assert_eq!(en(&err), (Class::Unreachable, unreachable.into()));
    }

    #[test]
    fn auth_failures_say_which_method_was_refused() {
        let auth = |keys, password| {
            anyhow::Error::new(Fail::Auth {
                user: "deploy".into(),
                host: "web-01".into(),
                keys,
                password,
            })
        };
        // A refused password is the news, however many keys went before it.
        for keys in [0, 3] {
            let wrong = "Wrong password for deploy@web-01";
            assert_eq!(en(&auth(keys, true)), (Class::Auth, wrong.into()));
        }
        let rejected = "Public key rejected for deploy@web-01: 2 offered, none accepted";
        assert_eq!(en(&auth(2, false)), (Class::Auth, rejected.into()));
        assert!(en(&auth(0, false))
            .1
            .starts_with("No way to log in as deploy@web-01:"));
    }

    #[test]
    fn host_key_mismatch_names_the_fingerprint_and_the_line() {
        let err = anyhow::Error::new(Fail::HostKey {
            host: "alpha".into(),
            fingerprint: "SHA256:abcDEF".into(),
            line: 7,
            file: "/home/me/.ssh/known_hosts".into(),
        });
        let (class, message) = en(&err);
        assert_eq!(class, Class::HostKey);
        assert_eq!(
            message,
            "Host key mismatch for alpha: the server sent SHA256:abcDEF, but line 7 of \
             /home/me/.ssh/known_hosts holds another key"
        );
        assert!(err.to_string().starts_with("HOST KEY MISMATCH for alpha: "));
    }

    #[test]
    fn a_failed_hop_is_named_with_its_own_reason() {
        let refused = Fail::Refused {
            addr: "10.0.0.1:22".into(),
        };
        let hop = "bastion (hop 1 of 2)".to_string();
        let err = os(ErrorKind::ConnectionRefused, "Connection refused").context(refused.clone());
        let err = err.context(Fail::Jump {
            hop: hop.clone(),
            cause: Some(Box::new(refused)),
        });
        let (class, message) = en(&err);
        assert_eq!(class, Class::Jump);
        assert_eq!(
            message,
            "Jump host bastion (hop 1 of 2) failed: Connection refused by 10.0.0.1:22 — is sshd running?"
        );
        assert_eq!(
            classify(&err, Lang::En).chain,
            "jump host bastion (hop 1 of 2) failed: connection refused by 10.0.0.1:22: Connection refused"
        );

        // The cause's own line of the chain is not repeated inside its message.
        let unreachable = Fail::Unreachable {
            addr: "10.0.0.9:22".into(),
        };
        let err = anyhow!("Failed to open channel (ConnectFailed)").context(unreachable.clone());
        let err = err.context(Fail::Jump {
            hop: hop.clone(),
            cause: Some(Box::new(unreachable)),
        });
        assert_eq!(
            en(&err).1,
            "Jump host bastion (hop 1 of 2) failed: Cannot reach 10.0.0.9:22: Failed to open channel (ConnectFailed)"
        );

        // A hop that failed for a reason nobody tagged still says which hop, and the reason as is.
        let err = anyhow!("channel refused").context(Fail::Jump { hop, cause: None });
        assert_eq!(
            en(&err).1,
            "Jump host bastion (hop 1 of 2) failed: channel refused"
        );
    }

    #[tokio::test]
    async fn library_errors_get_a_class_and_keep_their_own_words() {
        let refused = io::Error::new(ErrorKind::ConnectionRefused, "Connection refused");
        let err = anyhow::Error::new(russh::Error::IO(refused)).context("cannot open the forward");
        let raw = "cannot open the forward: Connection refused";
        assert_eq!(en(&err), (Class::Refused, raw.into()));

        let never = std::future::pending::<()>();
        let elapsed = tokio::time::timeout(std::time::Duration::ZERO, never).await;
        let err = anyhow::Error::new(elapsed.unwrap_err());
        assert_eq!(en(&err), (Class::Timeout, "deadline has elapsed".into()));
        let err = anyhow::Error::new(russh::Error::KeepaliveTimeout);
        assert_eq!(en(&err), (Class::Timeout, "Keepalive timeout".into()));

        let err = anyhow::Error::new(russh::Error::KeyChanged { line: 3 });
        assert_eq!(en(&err), (Class::HostKey, "Key changed, line 3".into()));
        let err = anyhow::Error::new(russh::Error::NoAuthMethod);
        assert_eq!(en(&err).0, Class::Auth);
    }

    #[test]
    fn an_unknown_error_is_shown_verbatim() {
        let err = anyhow!("Serial is not implemented yet").context("cannot open web-01");
        let raw = "cannot open web-01: Serial is not implemented yet";
        assert_eq!(en(&err), (Class::Other, raw.into()));
        // Not every io or russh error is a connection class.
        let err = os(
            ErrorKind::PermissionDenied,
            "Permission denied (os error 13)",
        );
        assert_eq!(en(&err).0, Class::Other);
        let err = anyhow::Error::new(russh::Error::Disconnect);
        assert_eq!(en(&err), (Class::Other, "Disconnected".into()));
        assert_eq!(Class::Other.label(Lang::En), "ERROR");
    }
}
