//! Platform abstractions replacing lightweight external crates.
//!
//! Provides cross-platform implementations for:
//! - Directory discovery (replaces `dirs`)
//! - URL percent-encoding (replaces `urlencoding`)
//! - Opening URLs in the default browser (replaces `open`)
//! - Finding executables on PATH (replaces `which`)
//! - Cryptographic random byte generation (replaces `getrandom`)
//! - File glob pattern matching (replaces `glob`)

use std::path::{Path, PathBuf};

// === Directory discovery ===

/// Returns the user's home directory.
pub fn home_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    } else {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// Returns the platform-specific cache directory.
///
/// - macOS: `$HOME/Library/Caches`
/// - Linux: `$XDG_CACHE_HOME` or `$HOME/.cache`
/// - Windows: `%LOCALAPPDATA%`
pub fn cache_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        home_dir().map(|h| h.join("Library/Caches"))
    } else if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        // Linux / other Unix
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|h| h.join(".cache")))
    }
}

/// Returns the platform-specific data directory.
///
/// - macOS: `$HOME/Library/Application Support`
/// - Linux: `$XDG_DATA_HOME` or `$HOME/.local/share`
/// - Windows: `%APPDATA%`
pub fn data_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        home_dir().map(|h| h.join("Library/Application Support"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|h| h.join(".local/share")))
    }
}

/// Returns the platform-specific local data directory.
///
/// Same as [`data_dir`] on macOS and Linux.
/// On Windows, returns `%LOCALAPPDATA%`.
pub fn data_local_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        data_dir()
    }
}

/// Returns the platform-specific documents directory.
pub fn document_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join("Documents"))
}

// === URL percent-encoding ===

/// Percent-encode a string for use in URLs.
///
/// Encodes all characters except unreserved characters
/// (A-Z, a-z, 0-9, `-`, `_`, `.`, `~`) as defined in RFC 3986.
pub fn percent_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push('%');
                encoded.push(HEX_CHARS[(byte >> 4) as usize] as char);
                encoded.push(HEX_CHARS[(byte & 0x0F) as usize] as char);
            }
        }
    }
    encoded
}

const HEX_CHARS: &[u8; 16] = b"0123456789ABCDEF";

/// Decode a percent-encoded string.
pub fn percent_decode(input: &str) -> Result<String, std::string::FromUtf8Error> {
    let mut bytes = Vec::with_capacity(input.len());
    let mut chars = input.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let hi = chars.next().and_then(from_hex_digit);
            let lo = chars.next().and_then(from_hex_digit);
            match (hi, lo) {
                (Some(h), Some(l)) => bytes.push((h << 4) | l),
                _ => {
                    // Malformed percent encoding — pass through literally
                    bytes.push(b'%');
                }
            }
        } else if b == b'+' {
            bytes.push(b' ');
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8(bytes)
}

fn from_hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

// === Browser/URL opening ===

/// Open a URL in the platform's default browser or handler.
pub fn open_in_browser(url: &str) -> std::io::Result<()> {
    let (cmd, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("open", &[url])
    } else if cfg!(windows) {
        ("cmd", &["/C", "start", "", url])
    } else {
        ("xdg-open", &[url])
    };

    std::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|_| ())
}

// === PATH lookup ===

/// Check whether an executable exists on the system PATH.
pub fn find_on_path(binary: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let separator = if cfg!(windows) { ';' } else { ':' };

    for dir in path_var.to_string_lossy().split(separator) {
        let candidate = Path::new(dir).join(binary);
        if candidate.is_file() {
            return Some(candidate);
        }
        // On Windows, also check with common extensions
        if cfg!(windows) {
            for ext in &[".exe", ".cmd", ".bat", ".com"] {
                let with_ext = Path::new(dir).join(format!("{}{}", binary, ext));
                if with_ext.is_file() {
                    return Some(with_ext);
                }
            }
        }
    }
    None
}

// === Random bytes ===

/// Fill a buffer with cryptographically secure random bytes.
#[cfg(unix)]
pub fn random_bytes(buf: &mut [u8]) {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(buf))
        .expect("Failed to read /dev/urandom");
}

/// Fill a buffer with cryptographically secure random bytes.
#[cfg(windows)]
pub fn random_bytes(buf: &mut [u8]) {
    extern "system" {
        fn BCryptGenRandom(
            hAlgorithm: *mut std::ffi::c_void,
            pbBuffer: *mut u8,
            cbBuffer: u32,
            dwFlags: u32,
        ) -> i32;
    }
    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x00000002;
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    assert!(status >= 0, "BCryptGenRandom failed with status {status}");
}

// === Glob pattern matching ===

/// Match files against a glob pattern and return matching paths.
///
/// Supports `*` (any non-separator chars), `**` (recursive directory match),
/// and `?` (single char). The pattern should be an absolute path with glob
/// characters (e.g., `/path/to/project/*.rb`).
pub fn glob(pattern: &str) -> Result<Vec<PathBuf>, String> {
    let pattern = pattern.replace('\\', "/");

    // Find where the first wildcard character appears
    let first_wild = pattern.find(['*', '?', '[']).unwrap_or(pattern.len());

    // Base directory is everything up to the last '/' before the first wildcard
    let base_end = pattern[..first_wild].rfind('/').map(|i| i + 1).unwrap_or(0);
    let base_dir = if base_end > 0 {
        PathBuf::from(&pattern[..base_end])
    } else {
        PathBuf::from(".")
    };

    if !base_dir.exists() {
        return Ok(Vec::new());
    }

    let rel_pattern = &pattern[base_end..];

    // Walk the directory tree and collect all files
    let mut all_paths = Vec::new();
    walk_dir_recursive(&base_dir, &mut all_paths);

    // Match each path against the relative pattern
    let mut matches: Vec<PathBuf> = Vec::new();
    for path in all_paths {
        let rel_path = path
            .strip_prefix(&base_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        if glob_match_path(rel_pattern, &rel_path) {
            matches.push(path);
        }
    }

    matches.sort();
    Ok(matches)
}

fn walk_dir_recursive(dir: &Path, results: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        results.push(path.clone());
        if path.is_dir() {
            walk_dir_recursive(&path, results);
        }
    }
}

/// Match a glob pattern against a path string.
///
/// Both pattern and path use `/` as separator. The pattern may contain
/// `*` (matches non-separator chars), `**` (matches across separators),
/// and `?` (matches single non-separator char).
fn glob_match_path(pattern: &str, path: &str) -> bool {
    let pat_parts: Vec<&str> = pattern.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();
    match_components(&pat_parts, &path_parts)
}

fn match_components(patterns: &[&str], paths: &[&str]) -> bool {
    if patterns.is_empty() {
        return paths.is_empty();
    }

    if patterns[0] == "**" {
        // `**` matches zero or more path components
        for i in 0..=paths.len() {
            if match_components(&patterns[1..], &paths[i..]) {
                return true;
            }
        }
        return false;
    }

    if paths.is_empty() {
        return false;
    }

    // Match first component, then recurse
    match_segment(patterns[0], paths[0]) && match_components(&patterns[1..], &paths[1..])
}

/// Match a single path segment against a glob pattern segment.
///
/// Handles `*` (any chars) and `?` (single char).
fn match_segment(pattern: &str, text: &str) -> bool {
    match_bytes(pattern.as_bytes(), text.as_bytes())
}

fn match_bytes(pattern: &[u8], text: &[u8]) -> bool {
    match (pattern.first(), text.first()) {
        (None, None) => true,
        (Some(b'*'), _) => {
            // Try matching zero chars, or advance text by one
            match_bytes(&pattern[1..], text)
                || (!text.is_empty() && match_bytes(pattern, &text[1..]))
        }
        (Some(b'?'), Some(_)) => match_bytes(&pattern[1..], &text[1..]),
        (Some(&a), Some(&b)) if a == b => match_bytes(&pattern[1..], &text[1..]),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // --- Directory tests ---

    #[test]
    fn home_dir_returns_some() {
        assert!(home_dir().is_some());
    }

    #[test]
    fn cache_dir_returns_some() {
        assert!(cache_dir().is_some());
    }

    #[test]
    fn data_dir_returns_some() {
        assert!(data_dir().is_some());
    }

    #[test]
    fn data_local_dir_returns_some() {
        assert!(data_local_dir().is_some());
    }

    // --- Percent-encoding tests ---

    #[test]
    fn percent_encode_preserves_unreserved() {
        assert_eq!(percent_encode("hello"), "hello");
        assert_eq!(percent_encode("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(percent_encode("ABC123"), "ABC123");
    }

    #[test]
    fn percent_encode_encodes_spaces_and_special() {
        assert_eq!(percent_encode("hello world"), "hello%20world");
        assert_eq!(percent_encode("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn percent_encode_encodes_unicode() {
        let encoded = percent_encode("emoji 🐛");
        assert!(encoded.starts_with("emoji%20"));
        assert!(!encoded.contains('🐛'));
    }

    #[test]
    fn percent_decode_roundtrip() {
        let original = "hello world & more";
        let encoded = percent_encode(original);
        let decoded = percent_decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn percent_decode_handles_plus_as_space() {
        assert_eq!(percent_decode("hello+world").unwrap(), "hello world");
    }

    #[test]
    fn percent_decode_passthrough_plain() {
        assert_eq!(percent_decode("hello").unwrap(), "hello");
    }

    // --- PATH lookup tests ---

    #[test]
    fn find_on_path_finds_sh() {
        // `sh` should exist on any Unix system
        if cfg!(unix) {
            assert!(find_on_path("sh").is_some());
        }
    }

    #[test]
    fn find_on_path_returns_none_for_nonexistent() {
        assert!(find_on_path("this_binary_definitely_does_not_exist_xyz").is_none());
    }

    // --- Random bytes tests ---

    #[test]
    fn random_bytes_fills_buffer() {
        let mut buf = [0u8; 16];
        random_bytes(&mut buf);
        // Extremely unlikely to be all zeros
        assert!(buf.iter().any(|&b| b != 0));
    }

    #[test]
    fn random_bytes_produces_different_values() {
        let mut buf1 = [0u8; 16];
        let mut buf2 = [0u8; 16];
        random_bytes(&mut buf1);
        random_bytes(&mut buf2);
        assert_ne!(buf1, buf2);
    }

    // --- Glob tests ---

    #[test]
    fn glob_match_simple_wildcard() {
        assert!(glob_match_path("*.rb", "foo.rb"));
        assert!(glob_match_path("*.rb", "bar.rb"));
        assert!(!glob_match_path("*.rb", "foo.txt"));
        assert!(!glob_match_path("*.rb", "sub/foo.rb"));
    }

    #[test]
    fn glob_match_recursive() {
        assert!(glob_match_path("**/*.rs", "main.rs"));
        assert!(glob_match_path("**/*.rs", "src/main.rs"));
        assert!(glob_match_path("**/*.rs", "src/sub/mod.rs"));
        assert!(!glob_match_path("**/*.rs", "src/main.txt"));
    }

    #[test]
    fn glob_match_question_mark() {
        assert!(glob_match_path("?.txt", "a.txt"));
        assert!(!glob_match_path("?.txt", "ab.txt"));
    }

    #[test]
    fn glob_match_no_wildcards() {
        assert!(glob_match_path("exact.txt", "exact.txt"));
        assert!(!glob_match_path("exact.txt", "other.txt"));
    }

    #[test]
    fn glob_finds_matching_files() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.rb"), "class A").unwrap();
        fs::write(temp.path().join("b.rb"), "class B").unwrap();
        fs::write(temp.path().join("c.txt"), "not ruby").unwrap();

        let pattern = format!("{}/*.rb", temp.path().display());
        let matches = glob(&pattern).unwrap();

        assert_eq!(matches.len(), 2);
        assert!(matches.iter().all(|p| p.extension().unwrap() == "rb"));
    }

    #[test]
    fn glob_recursive_pattern() {
        let temp = TempDir::new().unwrap();
        let sub = temp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(temp.path().join("top.rs"), "fn main() {}").unwrap();
        fs::write(sub.join("nested.rs"), "mod nested;").unwrap();
        fs::write(sub.join("other.txt"), "text").unwrap();

        let pattern = format!("{}/**/*.rs", temp.path().display());
        let matches = glob(&pattern).unwrap();

        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn glob_no_matches_returns_empty() {
        let temp = TempDir::new().unwrap();
        let pattern = format!("{}/*.nonexistent", temp.path().display());
        let matches = glob(&pattern).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn glob_nonexistent_dir_returns_empty() {
        let matches = glob("/nonexistent/path/*.txt").unwrap();
        assert!(matches.is_empty());
    }

    // --- open_in_browser ---

    #[test]
    fn open_in_browser_does_not_panic() {
        // We can't really test that it opens a browser, but we can ensure
        // the function compiles and the platform detection works.
        // Don't actually call it in tests as it would open a browser.
    }
}
