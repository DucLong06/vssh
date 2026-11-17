//! The line between the core and a frontend: work is asked for through
//! `task`, the answers arrive through the frontend's `Sink`, and the frontend
//! itself never spawns, awaits or times anything. These tests are such a
//! frontend: plain threads and a std channel, no async in sight.

use std::net::TcpListener;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::Duration;

use vssh_core::config::Lang;
use vssh_core::conn_err::{classify, Class};
use vssh_core::ssh::{Login, Target};
use vssh_core::task::{self, Event, Sink};

const WAIT: Duration = Duration::from_secs(10);

fn sink() -> (impl Sink, Receiver<Event>) {
    let (tx, rx) = channel();
    (move |ev| drop(tx.send(ev)), rx)
}

fn target(port: u16) -> Target {
    Target::parse(&format!("nobody@127.0.0.1:{port}")).unwrap()
}

/// A loopback port nothing listens on.
fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

#[test]
fn pings_answer_through_the_sink() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let open = listener.local_addr().unwrap().port();
    let mut answers = task::run(|| {
        let (sink, rx) = sink();
        task::ping("up".into(), target(open), sink.clone());
        task::ping("down".into(), target(closed_port()), sink);
        [rx.recv_timeout(WAIT), rx.recv_timeout(WAIT)].map(|ev| match ev {
            Ok(Event::Ping(alias, ms)) => (alias, ms.is_some()),
            _ => panic!("expected two pings"),
        })
    })
    .unwrap();
    answers.sort();
    assert_eq!(answers, [("down".into(), false), ("up".into(), true)]);
}

#[test]
fn a_session_that_cannot_connect_ends_with_its_error() {
    task::run(|| {
        let (sink, rx) = sink();
        let login = Login {
            command: None,
            script: None,
        };
        let (_task, _keys) =
            task::session(7, target(closed_port()), None, (80, 24), login, None, sink);
        match rx.recv_timeout(WAIT) {
            Ok(Event::Closed(7, Some(err))) => {
                assert_eq!(classify(&err, Lang::En).class, Class::Refused, "{err:#}")
            }
            _ => panic!("expected the session to end with an error"),
        }
        // That was the task's last word: its sink is gone with it.
        assert!(matches!(
            rx.recv_timeout(WAIT),
            Err(RecvTimeoutError::Disconnected)
        ));
    })
    .unwrap();
}

#[test]
fn an_aborted_task_says_nothing_more() {
    // Accepts, then never speaks SSH: a connection that would hang.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    task::run(|| {
        let (sink, rx) = sink();
        let task = task::batch(
            1,
            "mute".into(),
            Ok(target(port)),
            None,
            "true".into(),
            sink,
        );
        task.abort();
        assert!(matches!(
            rx.recv_timeout(WAIT),
            Err(RecvTimeoutError::Disconnected)
        ));
    })
    .unwrap();
}
