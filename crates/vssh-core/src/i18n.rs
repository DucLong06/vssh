//! The words a window shows, both languages side by side, so a string cannot
//! be added in one of them only. The desktop app reads this table as JSON
//! (`crates/vssh-desktop/src/lib/strings.json`, held equal to it by a test).
//! The Vietnamese terms are the TUI's: máy chủ, thẻ, phiên, kết nối nhanh,
//! chưa kiểm tra, mất kết nối, đoạn lệnh. `{n}` in a text is a number the window puts in,
//! `{name}` a file's name.

use crate::config::Lang;

/// Key, English, Vietnamese.
pub const STRINGS: &[(&str, &str, &str)] = &[
    ("hosts", "Hosts", "Máy chủ"),
    ("tags", "Tags", "Thẻ"),
    ("snippets", "Snippets", "Đoạn lệnh"),
    ("sessions", "Sessions", "Phiên"),
    ("no_hosts", "No hosts yet.", "Chưa có máy chủ nào."),
    (
        "no_hosts_hint",
        "Add them to ~/.ssh/config, or connect once without saving.",
        "Thêm vào ~/.ssh/config, hoặc kết nối một lần mà không lưu.",
    ),
    ("no_tags", "no tags", "chưa có thẻ"),
    ("pinned", "pinned", "đã ghim"),
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
    (
        "streamer_mode",
        "Streamer mode (made-up addresses)",
        "Chế độ phát trực tiếp (địa chỉ giả)",
    ),
    // What the core calls a failure it has no class for; the window's own, for one the core never saw.
    ("error", "ERROR", "LỖI"),
    ("theme", "Theme", "Giao diện"),
    ("theme_night", "Night", "Tối"),
    ("theme_day", "Day", "Sáng"),
    // The dashboard. A host's states are the TUI's words for them.
    ("not_checked", "not checked", "chưa kiểm tra"),
    ("down", "down", "mất kết nối"),
    ("asking", "asking", "đang hỏi"),
    ("mem", "MEM", "RAM"),
    ("search_hosts", "Search hosts, #tag", "Tìm máy chủ, #thẻ"),
    ("group_by_tag", "Group by tag", "Nhóm theo thẻ"),
    ("sort", "Sort", "Sắp xếp"),
    ("sort_order", "My order", "Thứ tự của tôi"),
    ("sort_name", "Name", "Tên"),
    ("sort_latency", "Latency", "Độ trễ"),
    // The quick peek: the rows and the keys of the TUI's.
    ("quick_peek", "Quick peek", "Xem nhanh"),
    ("connection", "CONNECTION", "KẾT NỐI"),
    ("reachable", "reachable", "kết nối được"),
    ("unreachable", "unreachable", "không kết nối được"),
    ("hostname", "Host", "Tên máy"),
    ("system", "System", "Hệ thống"),
    ("uptime", "Uptime", "Đã chạy"),
    ("load", "Load", "Tải"),
    ("disk", "Disk /", "Đĩa /"),
    ("refresh", "refresh", "làm mới"),
    ("try_again", "try again", "thử lại"),
    ("secs_ago", "{n}s ago", "{n} giây trước"),
    // The terminal tabs.
    ("connecting", "connecting", "đang kết nối"),
    ("connected", "connected", "đã kết nối"),
    ("disconnected", "disconnected", "mất kết nối"),
    ("reconnect", "Reconnect", "Kết nối lại"),
    ("running", "running", "đang chạy"),
    ("selected", "selected", "đã chọn"),
    // Broadcast, in the TUI's word for it.
    ("broadcast", "broadcast", "phát"),
    ("broadcasting", "Broadcasting: {n}", "Đang phát: {n}"),
    // A host key that is not the one on record.
    ("fingerprint", "Fingerprint", "Dấu vân tay"),
    ("on_record", "On record", "Đang lưu"),
    (
        "host_key_warning",
        "Someone may be listening in, or the host was reinstalled. Accept only if you know why its key changed.",
        "Có thể có kẻ nghe lén, hoặc máy chủ vừa được cài lại. Chỉ chấp nhận nếu bạn biết vì sao khoá đổi.",
    ),
    ("accept_once", "Accept once", "Chấp nhận một lần"),
    ("trust", "Trust", "Tin cậy"),
    // The file view. The panes are named as the TUI names them.
    ("files", "Files", "Tệp"),
    ("local", "local", "máy mình"),
    ("path", "Path", "Đường dẫn"),
    ("size", "Size", "Kích thước"),
    ("modified", "Modified", "Sửa đổi"),
    ("empty_dir", "Empty directory.", "Thư mục trống."),
    ("hidden_files", "Hidden files", "Tệp ẩn"),
    ("filter", "Filter", "Lọc"),
    ("upload", "Upload", "Tải lên"),
    ("download", "Download", "Tải xuống"),
    ("copy", "Copy", "Sao chép"),
    ("new_folder", "New folder", "Tạo thư mục"),
    ("rename", "Rename", "Đổi tên"),
    ("delete", "Delete", "Xoá"),
    ("delete_ask", "Delete {name}?", "Xoá {name}?"),
    (
        "overwrite_ask",
        "{name} exists on the other side. Overwrite?",
        "{name} đã có ở bên kia. Ghi đè?",
    ),
    ("overwrite", "Overwrite", "Ghi đè"),
    ("exists", "{name} already exists", "{name} đã tồn tại"),
    (
        "files_only",
        "Only files can be copied.",
        "Chỉ chép được tệp.",
    ),
    ("transfers", "Transfers", "Truyền tệp"),
    ("waiting", "waiting", "đang chờ"),
    // `{done}` and `{total}` are sizes, `{name}` whose side the part is on.
    (
        "cancelled_left",
        "cancelled · {done} of {total} left on {name}",
        "đã huỷ · còn {done} / {total} trên {name}",
    ),
    // Snippets and batches. "Run on" is the title of the TUI's batch prompt.
    ("run_on", "Run on {n} host(s)", "Chạy trên {n} máy"),
    ("command", "Command", "Lệnh"),
    ("run", "Run", "Chạy"),
    ("add", "Add", "Thêm"),
    ("edit", "Edit", "Sửa"),
    ("save", "Save", "Lưu"),
    ("all_hosts", "all hosts", "mọi máy chủ"),
    ("no_snippets", "No snippets yet.", "Chưa có đoạn lệnh nào."),
    // `{name}` is a parameter of a snippet: what stands between its `{{ }}`.
    ("param_hint", "value for {name}", "giá trị cho {name}"),
    ("no_output", "(no output)", "(không có kết quả)"),
    ("scroll", "scroll", "cuộn"),
    ("copy_all", "Copy all", "Chép tất cả"),
    // The TUI's word for what Ctrl-A y did.
    ("copied", "copied", "đã chép"),
    // A host's port forwards, in the TUI's word for them.
    ("forwards", "Forwards", "Chuyển tiếp"),
    ("no_forwards", "No forwards yet.", "Chưa có chuyển tiếp nào."),
    ("kind", "Kind", "Loại"),
    ("local_port", "Local port", "Cổng cục bộ"),
    ("remote", "Remote", "Đích"),
    ("status", "Status", "Trạng thái"),
    ("on", "On", "Bật"),
    ("off", "Off", "Tắt"),
    ("starting", "starting", "đang bật"),
    ("try_port", "Try port {n}", "Thử cổng {n}"),
    // The vault, in the TUI's words for it and its states.
    ("vault", "Vault", "Kho"),
    ("vault_locked", "locked", "đang khoá"),
    ("vault_unlocked", "unlocked", "đã mở"),
    (
        "vault_none",
        "There is no vault yet. Its passphrase will protect the passwords of your hosts.",
        "Chưa có kho. Mật khẩu kho sẽ bảo vệ mật khẩu các máy chủ của bạn.",
    ),
    ("passphrase", "Vault passphrase", "Mật khẩu kho"),
    ("repeat_passphrase", "Repeat passphrase", "Nhập lại mật khẩu"),
    ("unlock", "Unlock", "Mở kho"),
    ("create_vault", "Create vault", "Tạo kho"),
    (
        "passphrase_mismatch",
        "passphrases do not match",
        "hai lần nhập không khớp",
    ),
    (
        "passphrase_required",
        "The passphrase must not be empty",
        "Mật khẩu kho không được để trống",
    ),
    (
        "wrong_passphrase",
        "Wrong vault passphrase, or the vault file is damaged",
        "Sai mật khẩu kho, hoặc tệp kho bị hỏng",
    ),
    (
        "wrong_attempts",
        "{n} wrong attempts. The vault does not lock you out, but a forgotten passphrase cannot be recovered.",
        "Đã sai {n} lần. Kho không khoá bạn lại, nhưng mật khẩu đã quên thì không lấy lại được.",
    ),
    ("passwords", "Passwords", "Mật khẩu"),
    ("change_passphrase", "Change passphrase", "Đổi mật khẩu kho"),
    ("old_passphrase", "Current passphrase", "Mật khẩu hiện tại"),
    ("new_passphrase", "New passphrase", "Mật khẩu mới"),
    (
        "passphrase_changed",
        "Passphrase changed.",
        "Đã đổi mật khẩu kho.",
    ),
    ("export_backup", "Export backup", "Xuất bản sao lưu"),
    (
        "export_note",
        "An encrypted copy, opened by the same passphrase.",
        "Bản sao đã mã hoá, mở bằng chính mật khẩu kho.",
    ),
    ("exported", "Saved to {name}", "Đã lưu vào {name}"),
    // The host form: the titles are the TUI's.
    ("add_host", "Add host", "Thêm máy chủ"),
    ("edit_host", "Edit host", "Sửa máy chủ"),
    ("alias", "Alias", "Tên gọi"),
    ("sign_in_with", "Sign in with", "Đăng nhập bằng"),
    ("auth_password", "Vault password", "Mật khẩu trong kho"),
    ("auth_key", "Key", "Khoá"),
    ("auth_agent", "Agent", "Agent"),
    ("password", "Password", "Mật khẩu"),
    ("password_stored", "password stored", "đã lưu mật khẩu"),
    (
        "vault_needed",
        "Unlock the vault to store a password.",
        "Mở kho để lưu mật khẩu.",
    ),
    ("key_file", "Key file", "Tệp khoá"),
    ("jump_host", "Jump host", "Máy trung gian"),
    ("none", "none", "không"),
    ("group", "Group", "Nhóm"),
    ("notes", "Notes", "Ghi chú"),
    ("add_tag", "add a tag", "thêm thẻ"),
    (
        "alias_invalid",
        "The alias must be one word without wildcards.",
        "Tên gọi phải là một từ, không có ký tự đại diện.",
    ),
    (
        "host_required",
        "The host must not be empty.",
        "Chưa nhập máy chủ.",
    ),
    (
        "port_invalid",
        "The port must be a number from 1 to 65535.",
        "Cổng phải là số từ 1 đến 65535.",
    ),
    (
        "delete_host_note",
        "Its open sessions are closed. Its snippets, its forwards and its stored password go with it.",
        "Các phiên đang mở của máy sẽ đóng. Đoạn lệnh, chuyển tiếp và mật khẩu đã lưu của máy cũng bị xoá.",
    ),
    (
        "import_ssh_config",
        "Import from ssh config",
        "Nhập từ ssh config",
    ),
    (
        "import_note",
        "Hosts you already have are left as they are.",
        "Máy chủ đã có được giữ nguyên.",
    ),
    (
        "import_none",
        "No new hosts in that file.",
        "Tệp đó không có máy chủ mới.",
    ),
    ("import_n", "Import {n}", "Nhập {n}"),
    ("preview", "Preview", "Xem trước"),
    ("save_this_host", "Save this host", "Lưu máy chủ này"),
    // The key setup: its steps as they are done on the host.
    ("key_setup", "Set up SSH key", "Cài khoá SSH"),
    (
        "key_setup_note",
        "It needs root on the host, or sudo that asks no password.",
        "Cần quyền root trên máy chủ, hoặc sudo không hỏi mật khẩu.",
    ),
    ("key_step_generate", "Create an Ed25519 key", "Tạo khoá Ed25519"),
    (
        "key_step_generate_note",
        "A new key for this host alone, kept next to your ssh config.",
        "Một khoá mới dành riêng cho máy này, nằm cạnh tệp ssh config của bạn.",
    ),
    (
        "key_step_install",
        "Add it to authorized_keys",
        "Thêm vào authorized_keys",
    ),
    (
        "key_step_install_note",
        "sshd_config is backed up first, then the public key goes on the host.",
        "Sao lưu sshd_config trước, rồi đưa khoá công khai lên máy chủ.",
    ),
    (
        "key_step_verify",
        "Sign in with the new key",
        "Thử đăng nhập bằng khoá mới",
    ),
    (
        "key_step_verify_note",
        "A second connection, with the key and nothing else.",
        "Mở một kết nối thứ hai, chỉ dùng khoá.",
    ),
    (
        "key_step_disable",
        "Turn off password login",
        "Tắt đăng nhập bằng mật khẩu",
    ),
    (
        "key_step_disable_note",
        "Only now that the key is known to work.",
        "Chỉ làm khi đã chắc khoá dùng được.",
    ),
    (
        "key_setup_restored",
        "sshd_config was restored as it was — your access is unchanged",
        "Đã khôi phục sshd_config như cũ — truy cập của bạn không đổi",
    ),
    (
        "key_setup_done",
        "{name} is signed in to with its key from now on.",
        "Từ giờ {name} đăng nhập bằng khoá.",
    ),
    ("start", "Start", "Bắt đầu"),
    // The keys: the cheatsheet's own words, and what a key does where no
    // button says it. The names of keys are the same in both languages.
    ("shortcuts", "Keyboard shortcuts", "Phím tắt"),
    ("search_shortcuts", "Search shortcuts", "Tìm phím tắt"),
    (
        "no_shortcut",
        "No shortcut matches.",
        "Không có phím tắt nào khớp.",
    ),
    ("keys_anywhere", "Anywhere", "Mọi nơi"),
    (
        "keys_session",
        "In a session, after Ctrl+A",
        "Trong phiên, sau Ctrl+A",
    ),
    ("keys_results", "Command results", "Kết quả lệnh"),
    // In the status bar while Ctrl+A waits for its key.
    (
        "leader_waiting",
        "waiting for the second key — ? lists them",
        "đang chờ phím thứ hai — bấm ? để xem danh sách",
    ),
    (
        "new_tab",
        "New tab: quick connect",
        "Tab mới: kết nối nhanh",
    ),
    ("close_tab", "Close tab", "Đóng tab"),
    ("next_tab", "Next tab", "Tab kế tiếp"),
    ("prev_tab", "Previous tab", "Tab trước"),
    ("go_to_tab", "Go to tab", "Tới tab số"),
    ("back_to_hosts", "Back to the hosts", "Về bảng máy chủ"),
    (
        "copy_output",
        "Copy the last command's output",
        "Chép kết quả lệnh vừa chạy",
    ),
    (
        "broadcast_tab",
        "Broadcast on this tab, on or off",
        "Bật hoặc tắt phát ở tab này",
    ),
    (
        "broadcast_all",
        "Broadcast on every tab, on or off",
        "Bật hoặc tắt phát ở mọi tab",
    ),
    (
        "send_to_shell",
        "Send Ctrl+A to the shell (Ctrl+W and Ctrl+K too)",
        "Gửi Ctrl+A cho shell (cả Ctrl+W và Ctrl+K)",
    ),
    (
        "run_command",
        "Run a command on the selected hosts",
        "Chạy lệnh trên các máy đã chọn",
    ),
    ("reorder", "Move the host among the others", "Đổi chỗ máy chủ"),
    ("move", "Move up and down", "Lên, xuống"),
    ("first_last", "First, last", "Đầu, cuối"),
    ("open_folder", "Open folder", "Mở thư mục"),
    ("parent_folder", "Parent folder", "Thư mục cha"),
    ("right_click", "Right-click", "Chuột phải"),
    // The window's own shape: panes side by side, and nothing but the tabs.
    ("split_right", "Split right", "Chia đôi sang phải"),
    ("split_down", "Split down", "Chia đôi xuống dưới"),
    (
        "unsplit",
        "Unsplit: this pane back to a tab of its own",
        "Bỏ chia: đưa ô này về tab riêng",
    ),
    (
        "swap_panes",
        "Swap with the pane before, after",
        "Đổi chỗ với ô trước, ô sau",
    ),
    // Short: the palette finds a row by the letters of its name, in order.
    ("focus_mode", "Focus mode", "Chế độ tập trung"),
    ("hide_sidebar", "Hide the sidebar", "Ẩn thanh bên"),
    ("show_sidebar", "Show the sidebar", "Hiện thanh bên"),
    ("rename_tab", "Rename tab", "Đổi tên tab"),
    // What the pointer does that no key says.
    ("keys_mouse", "With the mouse", "Bằng chuột"),
    ("middle_click", "Middle-click", "Chuột giữa"),
    ("double_click", "Double-click", "Nhấp đúp"),
    ("drag", "Drag", "Kéo thả"),
    (
        "drag_tab",
        "A tab onto an edge of what is open: split there, or change places",
        "Tab vào một mép của ô đang mở: chia ở phía đó, hoặc đổi chỗ",
    ),
    // A forward's kind in words: ssh's L, R and D.
    ("fwd_local", "Local", "Cục bộ"),
    ("fwd_remote", "Remote", "Từ xa"),
    // Before a tab goes whose shell is in the middle of something.
    (
        "close_busy",
        "{command} is running on {name}. Close the tab?",
        "{command} đang chạy trên {name}. Đóng tab?",
    ),
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
