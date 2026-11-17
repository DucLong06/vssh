//! The status line's share of the server metrics: how many of them fit.

use vssh_core::stats::{width, Level, Stats};

use super::panes::Shape;

/// What the status line shows in `room` cells: `CPU 23% · MEM 4.2/16G ·
/// / 61%`, with a one-cell bar per metric if those fit too, and metrics
/// dropped from the end if even the plain form does not. Nothing outside
/// landscape: a portrait or small status line keeps to mode and host.
pub fn compact(
    stats: &Stats,
    mem_label: &str,
    room: usize,
    shape: Shape,
) -> Vec<(String, String, Level)> {
    if shape != Shape::Wide {
        return vec![];
    }
    let with_bars = stats.parts(mem_label, true);
    if width(&with_bars) <= room {
        return with_bars;
    }
    let mut plain = stats.parts(mem_label, false);
    while width(&plain) > room {
        plain.pop();
    }
    plain
}

#[cfg(test)]
mod tests {
    use vssh_core::stats::parse;

    use super::*;

    const LINUX: &str = "load 0.92 0.58 0.59 1/389 12345\n\
        cpus 4\n\
        MemTotal:       16303428 kB\n\
        MemAvailable:   11883116 kB\n\
        /dev/nvme0n1p2   487652000 278412000 184340000      61% /\n";

    fn text(parts: &[(String, String, Level)]) -> String {
        let joined: Vec<String> = parts.iter().map(|(l, v, _)| format!("{l} {v}")).collect();
        joined.join(" · ")
    }

    #[test]
    fn compact_text_fits_the_room() {
        let stats = parse(LINUX).unwrap();
        let show = |room| text(&compact(&stats, "MEM", room, Shape::Wide));
        let full = "CPU 23% · MEM 4.2/16G · / 61%";
        let bars = "CPU ▂ 23% · MEM ▂ 4.2/16G · / ▅ 61%";
        assert_eq!((full.chars().count(), bars.chars().count()), (29, 35));

        assert_eq!(show(80), bars);
        assert_eq!(show(35), bars);
        assert_eq!(show(34), full);
        assert_eq!(show(29), full);
        assert_eq!(show(28), "CPU 23% · MEM 4.2/16G");
        assert_eq!(show(20), "CPU 23%");
        assert_eq!(show(7), "CPU 23%");
        assert_eq!(show(6), "");
        assert_eq!(show(0), "");
        // Whatever is shown never takes more than the room it was given.
        for room in 0..60 {
            let parts = compact(&stats, "MEM", room, Shape::Wide);
            assert!(width(&parts) <= room, "{room}");
            assert_eq!(width(&parts), text(&parts).chars().count());
        }
        // The label is the caller's, so it can be translated.
        assert!(text(&compact(&stats, "RAM", 80, Shape::Wide)).contains("RAM ▂ 4.2/16G"));
    }

    #[test]
    fn portrait_and_small_windows_hide_the_stats() {
        let stats = parse(LINUX).unwrap();
        for shape in [Shape::Tall, Shape::Compact] {
            assert!(compact(&stats, "MEM", 200, shape).is_empty(), "{shape:?}");
        }
        assert_eq!(compact(&stats, "MEM", 200, Shape::Wide).len(), 3);
    }
}
