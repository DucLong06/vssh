//! The words a window shows, both languages side by side, so a string cannot
//! be added in one of them only. The desktop app reads this table as JSON
//! (`crates/vssh-desktop/src/lib/strings.json`, held equal to it by a test).
//! The Vietnamese terms are the TUI's: máy chủ, thẻ, phiên, kết nối nhanh,
//! chưa kiểm tra, mất kết nối.

use crate::config::Lang;

/// Key, English, Vietnamese.
pub const STRINGS: &[(&str, &str, &str)] = &[
    ("hosts", "Hosts", "Máy chủ"),
    ("tags", "Tags", "Thẻ"),
    ("snippets", "Snippets", "Snippet"),
    ("sessions", "Sessions", "Phiên"),
    ("no_hosts", "No hosts yet.", "Chưa có máy chủ nào."),
    (
        "no_hosts_hint",
        "Add them to ~/.ssh/config, or connect once without saving.",
        "Thêm vào ~/.ssh/config, hoặc kết nối một lần mà không lưu.",
    ),
    ("no_tags", "no tags", "chưa có thẻ"),
    ("pinned", "pinned", "đã ghim"),
    (
        "session_note",
        "Opened, with no terminal yet.",
        "Đã mở, chưa có terminal.",
    ),
    ("palette", "Command palette", "Bảng lệnh"),
    ("search", "Search hosts and actions", "Tìm máy chủ và lệnh"),
    (
        "no_match",
        "No host matches the search.",
        "Không có máy chủ nào khớp.",
    ),
    ("select", "select", "chọn"),
    ("close", "close", "đóng"),
    ("quick_connect", "Quick connect", "Kết nối nhanh"),
    (
        "quick_connect_note",
        "Nothing is saved.",
        "Không có gì được lưu.",
    ),
    ("host", "Host", "Máy chủ"),
    ("user", "User", "Người dùng"),
    ("port", "Port", "Cổng"),
    ("connect", "Connect", "Kết nối"),
    ("cancel", "Cancel", "Huỷ"),
    ("language", "Language", "Ngôn ngữ"),
    // The dashboard. A host's states are the TUI's words for them.
    ("not_checked", "not checked", "chưa kiểm tra"),
    ("down", "down", "mất kết nối"),
    ("asking", "asking", "đang hỏi"),
    ("mem", "MEM", "RAM"),
];

/// The text under `key` in this language; `None` for a key nobody defined.
pub fn text(key: &str, lang: Lang) -> Option<&'static str> {
    let row = STRINGS.iter().find(|(k, ..)| *k == key)?;
    Some(lang.pick(row.1, row.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_string_is_there_once_in_both_languages() {
        for (i, (key, en, vi)) in STRINGS.iter().enumerate() {
            let name = |c: char| c.is_ascii_lowercase() || c == '_';
            assert!(!key.is_empty() && key.chars().all(name), "{key:?}");
            assert!(!en.trim().is_empty() && !vi.trim().is_empty(), "{key}");
            assert!(STRINGS[..i].iter().all(|(k, ..)| k != key), "{key} twice");
        }
        assert_eq!(text("quick_connect", Lang::En), Some("Quick connect"));
        assert_eq!(text("quick_connect", Lang::Vi), Some("Kết nối nhanh"));
        assert_eq!(text("sessions", Lang::Vi), Some("Phiên"));
        assert_eq!(text("nope", Lang::En), None);
    }
}
