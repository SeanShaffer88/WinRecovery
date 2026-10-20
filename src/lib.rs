//! WinRecovery — pure logic core.
//!
//! Everything in this module is platform-independent and unit-tested on any
//! OS: JSON writing/escaping, restore-point parsing, `vssadmin` output
//! parsing, and the data types the C ABI returns as JSON. All Windows access
//! lives in [`platform`]; all C ABI surface lives in [`ffi`].

pub mod ffi;
pub mod platform;

use std::fmt::Write as _;

/// Library version, also returned by `wr_version()`.
pub const VERSION: &str = "0.1.0";
pub const VERSION_MAJOR: u32 = 0;
pub const VERSION_MINOR: u32 = 1;
pub const VERSION_PATCH: u32 = 0;

// ---------------------------------------------------------------------------
// Error codes (mirrored in include/winrecovery.h)
// ---------------------------------------------------------------------------

pub const WR_OK: i32 = 0;
pub const WR_ERR_INVALID_ARG: i32 = 1;
pub const WR_ERR_NOT_SUPPORTED: i32 = 2;
pub const WR_ERR_NOT_FOUND: i32 = 3;
pub const WR_ERR_ACCESS_DENIED: i32 = 4;
pub const WR_ERR_PLATFORM: i32 = 5;
pub const WR_ERR_INTERNAL: i32 = 6;

// ---------------------------------------------------------------------------
// JSON writing (hand-rolled; this crate has zero dependencies by design)
// ---------------------------------------------------------------------------

/// Escape a string for inclusion inside JSON double quotes.
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

fn js(out: &mut String, key: &str, val: &str, first: &mut bool) {
    if !*first {
        out.push(',');
    }
    *first = false;
    let _ = write!(out, "\"{}\":\"{}\"", key, json_escape(val));
}

fn jn(out: &mut String, key: &str, val: u64, first: &mut bool) {
    if !*first {
        out.push(',');
    }
    *first = false;
    let _ = write!(out, "\"{}\":{}", key, val);
}

fn jb(out: &mut String, key: &str, val: bool, first: &mut bool) {
    if !*first {
        out.push(',');
    }
    *first = false;
    let _ = write!(out, "\"{}\":{}", key, val);
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestorePoint {
    pub sequence: u32,
    pub description: String,
    /// Local time, `YYYY-MM-DD HH:MM` when the source timestamp parses.
    pub created: String,
    /// Raw Windows restore-point type code.
    pub kind_code: u32,
    pub kind: &'static str,
}

impl RestorePoint {
    pub fn kind_label(code: u32) -> &'static str {
        match code {
            0 => "install",
            1 => "uninstall",
            12 => "manual",
            17 | 18 => "update",
            _ => "system",
        }
    }
    pub fn to_json(&self) -> String {
        let mut s = String::from("{");
        let mut f = true;
        jn(&mut s, "sequence", self.sequence as u64, &mut f);
        js(&mut s, "description", &self.description, &mut f);
        js(&mut s, "created", &self.created, &mut f);
        js(&mut s, "kind", self.kind, &mut f);
        jn(&mut s, "kind_code", self.kind_code as u64, &mut f);
        s.push('}');
        s
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Protection {
    pub enabled: bool,
    pub used_bytes: Option<u64>,
    pub max_bytes: Option<u64>,
    pub drive: String,
}

impl Protection {
    pub fn to_json(&self) -> String {
        let mut s = String::from("{");
        let mut f = true;
        jb(&mut s, "enabled", self.enabled, &mut f);
        js(&mut s, "drive", &self.drive, &mut f);
        match self.used_bytes {
            Some(v) => jn(&mut s, "used_bytes", v, &mut f),
            None => {
                if !f {
                    s.push(',');
                }
                f = false;
                s.push_str("\"used_bytes\":null");
            }
        }
        match self.max_bytes {
            Some(v) => jn(&mut s, "max_bytes", v, &mut f),
            None => {
                if !f {
                    s.push(',');
                }
                s.push_str("\"max_bytes\":null");
            }
        }
        s.push('}');
        s
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecycleBinInfo {
    pub drive: String,
    pub item_count: u64,
    pub size_bytes: u64,
}

impl RecycleBinInfo {
    pub fn to_json(&self) -> String {
        let mut s = String::from("{");
        let mut f = true;
        js(&mut s, "drive", &self.drive, &mut f);
        jn(&mut s, "item_count", self.item_count, &mut f);
        jn(&mut s, "size_bytes", self.size_bytes, &mut f);
        s.push('}');
        s
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledProgram {
    pub key: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    /// `YYYY-MM-DD` when known, else empty.
    pub install_date: String,
    pub install_location: String,
    pub uninstall_string: String,
    pub size_kb: u64,
    /// `machine64`, `machine32`, or `user`.
    pub source: &'static str,
    pub system_component: bool,
}

impl InstalledProgram {
    pub fn to_json(&self) -> String {
        let mut s = String::from("{");
        let mut f = true;
        js(&mut s, "key", &self.key, &mut f);
        js(&mut s, "name", &self.name, &mut f);
        js(&mut s, "publisher", &self.publisher, &mut f);
        js(&mut s, "version", &self.version, &mut f);
        js(&mut s, "install_date", &self.install_date, &mut f);
        js(&mut s, "install_location", &self.install_location, &mut f);
        js(&mut s, "uninstall_string", &self.uninstall_string, &mut f);
        jn(&mut s, "size_kb", self.size_kb, &mut f);
        js(&mut s, "source", self.source, &mut f);
        jb(&mut s, "system_component", self.system_component, &mut f);
        s.push('}');
        s
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupEntry {
    pub name: String,
    pub command: String,
    /// Where it was found, e.g. `HKLM\...\Run` or a Startup folder path.
    pub location: String,
    /// `registry` or `folder`.
    pub source: &'static str,
}

impl StartupEntry {
    pub fn to_json(&self) -> String {
        let mut s = String::from("{");
        let mut f = true;
        js(&mut s, "name", &self.name, &mut f);
        js(&mut s, "command", &self.command, &mut f);
        js(&mut s, "location", &self.location, &mut f);
        js(&mut s, "source", self.source, &mut f);
        s.push('}');
        s
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceEntry {
    pub name: String,
    pub display_name: String,
    /// `stopped`, `start_pending`, `stop_pending`, `running`,
    /// `continue_pending`, `pause_pending`, `paused`, or `unknown`.
    pub state: &'static str,
    pub process_id: u32,
    /// True for kernel/file-system drivers, false for Win32 services.
    pub is_driver: bool,
}

impl ServiceEntry {
    pub fn state_label(code: u32) -> &'static str {
        match code {
            1 => "stopped",
            2 => "start_pending",
            3 => "stop_pending",
            4 => "running",
            5 => "continue_pending",
            6 => "pause_pending",
            7 => "paused",
            _ => "unknown",
        }
    }
    pub fn to_json(&self) -> String {
        let mut s = String::from("{");
        let mut f = true;
        js(&mut s, "name", &self.name, &mut f);
        js(&mut s, "display_name", &self.display_name, &mut f);
        js(&mut s, "state", self.state, &mut f);
        jn(&mut s, "process_id", self.process_id as u64, &mut f);
        jb(&mut s, "is_driver", self.is_driver, &mut f);
        s.push('}');
        s
    }
}

/// Join JSON objects into a JSON array string.
pub fn json_array(items: &[String]) -> String {
    let mut s = String::from("[");
    for (i, it) in items.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(it);
    }
    s.push(']');
    s
}

// ---------------------------------------------------------------------------
// Parsing helpers (pure; shared by the Windows platform layer)
// ---------------------------------------------------------------------------

/// `20261015083012.000000-000` → `2026-10-15 08:30`. Anything else passes
/// through unchanged.
pub fn wmi_time(s: &str) -> String {
    if s.len() >= 12 && s[..12].chars().all(|c| c.is_ascii_digit()) {
        format!(
            "{}-{}-{} {}:{}",
            &s[0..4],
            &s[4..6],
            &s[6..8],
            &s[8..10],
            &s[10..12]
        )
    } else {
        s.to_string()
    }
}

/// Minimal JSON field extraction for the flat objects PowerShell's
/// `ConvertTo-Json` emits for restore points. This is deliberately narrow:
/// it understands string and number values in a flat object/array of
/// objects, which is all `Get-ComputerRestorePoint` produces.
fn json_str_field(obj: &str, name: &str) -> Option<String> {
    let pat = format!("\"{}\"", name);
    let start = obj.find(&pat)? + pat.len();
    let rest = obj[start..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    if let Some(stripped) = rest.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = stripped.chars();
        loop {
            match chars.next()? {
                '\\' => match chars.next()? {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        let hex: String = chars.by_ref().take(4).collect();
                        if let Ok(n) = u32::from_str_radix(&hex, 16) {
                            if let Some(c) = char::from_u32(n) {
                                out.push(c);
                            }
                        }
                    }
                    c => out.push(c),
                },
                '"' => break,
                c => out.push(c),
            }
        }
        Some(out)
    } else {
        // Bare number / literal: read to the next delimiter.
        let end = rest
            .find(|c: char| [',', '}', ']'].contains(&c))
            .unwrap_or(rest.len());
        Some(rest[..end].trim().to_string())
    }
}

fn json_u32_field(obj: &str, name: &str) -> Option<u32> {
    json_str_field(obj, name)?.parse().ok()
}

/// Split a flat JSON array of objects into its object substrings.
fn split_objects(json: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start: Option<usize> = None;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in json.char_indices() {
        if in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start.take() {
                        out.push(&json[s..=i]);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Parse `Get-ComputerRestorePoint | ConvertTo-Json` output (single object
/// or array) into restore points, newest sequence first.
pub fn parse_restore_points(json: &str) -> Vec<RestorePoint> {
    let json = json.trim();
    if json.is_empty() {
        return Vec::new();
    }
    let mut v: Vec<RestorePoint> = split_objects(json)
        .into_iter()
        .filter_map(|obj| {
            let sequence = json_u32_field(obj, "SequenceNumber")?;
            let kind_code = json_u32_field(obj, "RestorePointType").unwrap_or(0);
            Some(RestorePoint {
                sequence,
                description: json_str_field(obj, "Description").unwrap_or_default(),
                created: wmi_time(&json_str_field(obj, "CreationTime").unwrap_or_default()),
                kind_code,
                kind: RestorePoint::kind_label(kind_code),
            })
        })
        .collect();
    v.sort_by_key(|p| std::cmp::Reverse(p.sequence));
    v
}

/// Parse a human size like `2.31 GB` (from `vssadmin`) into bytes.
pub fn vss_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let mut it = s.split_whitespace();
    let n: f64 = it.next()?.replace(',', ".").parse().ok()?;
    let unit = it.next()?.to_ascii_uppercase();
    let mult = match unit.as_str() {
        "B" | "BYTES" => 1.0,
        "KB" => 1024.0,
        "MB" => 1024.0 * 1024.0,
        "GB" => 1024.0 * 1024.0 * 1024.0,
        "TB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((n * mult) as u64)
}

/// Parse `vssadmin list shadowstorage` for one drive (`C:`):
/// returns (used_bytes, max_bytes).
pub fn parse_shadowstorage(text: &str, drive: &str) -> (Option<u64>, Option<u64>) {
    let drive = drive.to_ascii_uppercase();
    for block in text.split("Shadow Copy Storage association") {
        let for_drive = block.lines().any(|l| {
            l.trim_start().starts_with("For volume:")
                && l.to_ascii_uppercase().contains(&format!("({drive})"))
        });
        if !for_drive {
            continue;
        }
        let field = |name: &str| {
            block
                .lines()
                .find(|l| l.trim_start().starts_with(name))
                .and_then(|l| l.split_once(':'))
                .and_then(|(_, v)| vss_size(v))
        };
        return (
            field("Used Shadow Copy Storage space"),
            field("Maximum Shadow Copy Storage space"),
        );
    }
    (None, None)
}

/// Sanitize a restore-point description the way InstallScope does: keep it
/// short and free of characters PowerShell would treat specially.
pub fn sanitize_description(description: &str) -> String {
    description
        .chars()
        .filter(|c| c.is_alphanumeric() || " -_.()".contains(*c))
        .take(60)
        .collect()
}

/// Normalize an `InstallDate` registry value (`20261015`) to `2026-10-15`.
pub fn normalize_install_date(raw: &str) -> String {
    let raw = raw.trim();
    if raw.len() == 8 && raw.chars().all(|c| c.is_ascii_digit()) {
        format!("{}-{}-{}", &raw[0..4], &raw[4..6], &raw[6..8])
    } else {
        raw.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_json() {
        assert_eq!(json_escape("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
        assert_eq!(json_escape("plain"), "plain");
    }

    #[test]
    fn parses_restore_points_single_and_array() {
        let one = r#"{"SequenceNumber":7,"Description":"Before uninstall","CreationTime":"20261015083012.000000-000","RestorePointType":12}"#;
        let p = parse_restore_points(one);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].created, "2026-10-15 08:30");
        assert_eq!(p[0].kind, "manual");

        let many = r#"[{"SequenceNumber":3,"Description":"Windows Update","CreationTime":"20261001120000.000000-000","RestorePointType":18},{"SequenceNumber":9,"Description":"Installed \"Acme\"","CreationTime":"20261012090000.000000-000","RestorePointType":0}]"#;
        let p = parse_restore_points(many);
        assert_eq!(p.iter().map(|x| x.sequence).collect::<Vec<_>>(), vec![9, 3]);
        assert_eq!(p[0].description, "Installed \"Acme\"");
        assert_eq!(p[1].kind, "update");
        assert!(parse_restore_points("").is_empty());
        assert!(parse_restore_points("not json").is_empty());
    }

    #[test]
    fn parses_shadow_storage() {
        let text = "vssadmin 1.1\n\nShadow Copy Storage association\n   For volume: (C:)\\\\?\\Volume{abc}\\\n   Used Shadow Copy Storage space: 2.31 GB (1%)\n   Maximum Shadow Copy Storage space: 23.8 GB (10%)\n";
        let (used, max) = parse_shadowstorage(text, "C:");
        assert_eq!(used, Some((2.31 * 1073741824.0) as u64));
        assert_eq!(max, Some((23.8 * 1073741824.0) as u64));
        assert_eq!(parse_shadowstorage(text, "D:"), (None, None));
    }

    #[test]
    fn json_shapes_are_valid_looking() {
        let rp = RestorePoint {
            sequence: 5,
            description: "Test \"quoted\"".into(),
            created: "2026-10-20 12:00".into(),
            kind_code: 12,
            kind: "manual",
        };
        let j = rp.to_json();
        assert!(j.contains("\"sequence\":5"));
        assert!(j.contains("Test \\\"quoted\\\""));
        let arr = json_array(&[j]);
        assert!(arr.starts_with("[{") && arr.ends_with("}]"));

        let prot = Protection {
            enabled: true,
            used_bytes: None,
            max_bytes: Some(1024),
            drive: "C:".into(),
        };
        let j = prot.to_json();
        assert!(j.contains("\"used_bytes\":null"));
        assert!(j.contains("\"max_bytes\":1024"));
    }

    #[test]
    fn misc_helpers() {
        assert_eq!(wmi_time("20261015083012.000000-000"), "2026-10-15 08:30");
        assert_eq!(wmi_time("garbage"), "garbage");
        assert_eq!(normalize_install_date("20261015"), "2026-10-15");
        assert_eq!(normalize_install_date("unknown"), "unknown");
        assert_eq!(sanitize_description("a'b; $x `y`"), "ab x y");
        assert_eq!(ServiceEntry::state_label(4), "running");
        assert_eq!(ServiceEntry::state_label(99), "unknown");
    }
}
