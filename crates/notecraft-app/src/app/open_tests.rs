use super::is_openable;

#[test]
fn only_safe_targets_are_opened() {
    for ok in [
        "https://example.com/a?b=c&d=e",
        "HTTP://example.com",
        "mailto:a@b.ch",
        "tel:+41123",
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
        "/Users/me/notes/file.pdf",
    ] {
        assert!(is_openable(ok), "{ok}");
    }
    for bad in [
        "",
        "file:///etc/passwd",
        "smb://server/share",
        "ssh://host",
        "javascript:alert(1)",
        "-a Calculator",
        "--help",
        "../../etc/passwd",
        "relative/file.txt",
        "https://a.b\nfile:///x",
        "vscode://file/x",
        "/Applications/Calculator.app",
        "/tmp/run.command",
        "/tmp/Install.PKG",
    ] {
        assert!(!is_openable(bad), "{bad:?}");
    }
}
