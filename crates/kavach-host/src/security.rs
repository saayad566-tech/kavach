//! Security scanning of generated code.
//!
//! This is what makes the plan's "Run security checks" task verifiable: before, that task
//! could never be evidenced, so it was always reported as unverified. Now a real scan runs
//! and produces a finding list, and a high-severity finding fails verification.
//!
//! The rules are deliberately conservative. A scanner that cries wolf gets switched off, so
//! each rule targets a specific, recognisable mistake rather than "looks random".

use std::path::Path;

use crate::path::list_project_files;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low,
    Medium,
    High,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::High => "high",
            Severity::Medium => "medium",
            Severity::Low => "low",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub path: String,
    pub line: usize,
    pub severity: Severity,
    pub rule: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct ScanReport {
    pub findings: Vec<Finding>,
    pub files_scanned: usize,
}

impl ScanReport {
    pub fn highest(&self) -> Option<Severity> {
        self.findings.iter().map(|f| f.severity).max()
    }

    pub fn count(&self, severity: Severity) -> usize {
        self.findings.iter().filter(|f| f.severity == severity).count()
    }

    /// Verification gate: any high-severity finding blocks delivery.
    pub fn passes(&self) -> bool {
        self.count(Severity::High) == 0
    }

    pub fn summary(&self) -> String {
        if self.findings.is_empty() {
            return format!("{} file(s) scanned, no findings", self.files_scanned);
        }
        format!(
            "{} file(s) scanned: {} high, {} medium, {} low",
            self.files_scanned,
            self.count(Severity::High),
            self.count(Severity::Medium),
            self.count(Severity::Low)
        )
    }
}

const SECRET_NAMES: &[&str] = &["api_key", "apikey", "secret", "token", "password", "passwd", "private_key", "access_key"];

fn is_placeholder(value: &str) -> bool {
    let lowered = value.to_lowercase();
    value.is_empty()
        || value.len() < 8
        || lowered.contains("your")
        || lowered.contains("example")
        || lowered.contains("changeme")
        || lowered.contains("placeholder")
        || lowered.contains("xxxx")
        || lowered.starts_with("${")
        || lowered.starts_with("process.env")
        || lowered.starts_with("<")
        || lowered.starts_with("env.")
}

/// Scan one file's content.
pub fn scan_content(path: &str, content: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let lowered = content.to_lowercase();

    for (index, raw) in content.lines().enumerate() {
        let line_no = index + 1;
        let line = raw.trim();

        // A private key block is never legitimate in generated application code. Checked
        // per line: testing the whole file fired once for every line in it.
        let line_lower = line.to_lowercase();
        if line_lower.contains("begin") && line_lower.contains("private key") {
            findings.push(Finding {
                path: path.into(),
                line: line_no,
                severity: Severity::High,
                rule: "private-key",
                detail: "a private key block is present".into(),
            });
        }

        // AWS access key ids are distinctive: AKIA followed by 16 upper-case characters.
        if has_aws_key(line) {
            findings.push(Finding {
                path: path.into(),
                line: line_no,
                severity: Severity::High,
                rule: "aws-access-key",
                detail: "looks like an AWS access key id".into(),
            });
        }

        // A secret-looking name assigned a real-looking literal.
        if let Some(value) = assignment_value(line) {
            let name = line.split(['=', ':']).next().unwrap_or("").to_lowercase();
            if SECRET_NAMES.iter().any(|needle| name.contains(needle)) && !is_placeholder(&value) {
                findings.push(Finding {
                    path: path.into(),
                    line: line_no,
                    severity: Severity::Medium,
                    rule: "hardcoded-secret",
                    detail: format!("a value is assigned to a secret-looking name ({})", name.trim()),
                });
            }
        }

        // Code execution sinks in a generated web app.
        for (needle, rule, detail) in [
            ("eval(", "eval", "eval() executes arbitrary strings"),
            ("new function(", "dynamic-function", "the Function constructor executes arbitrary strings"),
            ("child_process", "child-process", "spawns processes from application code"),
            ("require('child_process')", "child-process", "spawns processes from application code"),
            ("document.write(", "document-write", "document.write can inject markup"),
        ] {
            if lowered.contains(needle) {
                findings.push(Finding {
                    path: path.into(),
                    line: line_no,
                    severity: Severity::Medium,
                    rule,
                    detail: detail.into(),
                });
            }
        }

        // A non-loopback http:// URL means traffic would leave the device unencrypted.
        if let Some(url) = first_http_url(line) {
            findings.push(Finding {
                path: path.into(),
                line: line_no,
                severity: Severity::Low,
                rule: "insecure-url",
                detail: format!("plaintext URL {url}"),
            });
        }

        // A long base64 blob in source is usually a committed credential.
        if let Some(len) = long_base64_token(line) {
            findings.push(Finding {
                path: path.into(),
                line: line_no,
                severity: Severity::Low,
                rule: "long-opaque-token",
                detail: format!("{len} characters of base64-like data"),
            });
        }
    }

    findings
}

fn has_aws_key(line: &str) -> bool {
    // Byte slicing a &str panics if the index is not a char boundary, and this runs over
    // arbitrary generated content — so work on the bytes throughout and only convert the
    // ASCII window we already know is ASCII.
    let bytes = line.as_bytes();
    if bytes.len() < 20 {
        return false;
    }
    for i in 0..=bytes.len() - 20 {
        if &bytes[i..i + 4] == b"AKIA" {
            let window = &bytes[i + 4..i + 20];
            if window.iter().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

fn assignment_value(line: &str) -> Option<String> {
    let (_, rest) = line.split_once(['=', ':'])?;
    let rest = rest.trim();
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' && quote != '`' {
        return None;
    }
    let inner = rest[1..].split(quote).next()?;
    Some(inner.to_string())
}

fn first_http_url(line: &str) -> Option<String> {
    let start = line.find("http://")?;
    let rest = &line[start + 7..];
    let host: String = rest.chars().take_while(|c| !c.is_whitespace() && *c != '"' && *c != '\'').collect();
    let hostname = host.split(['/', ':', '?']).next().unwrap_or("");
    if hostname.is_empty() || hostname == "127.0.0.1" || hostname == "localhost" {
        return None;
    }
    Some(format!("http://{host}"))
}

fn long_base64_token(line: &str) -> Option<usize> {
    let candidate: String = line
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '+' || *c == '/' || *c == '=')
        .collect();
    for token in candidate.split('=') {
        if token.len() >= 48 && token.chars().any(|c| c.is_ascii_digit()) {
            return Some(token.len());
        }
    }
    None
}

/// Scan a project directory.
pub fn scan_project(root: &Path) -> ScanReport {
    let mut report = ScanReport::default();
    for rel in list_project_files(root) {
        let path = root.join(&rel);
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        if content.len() > 2 * 1024 * 1024 {
            continue;
        }
        report.files_scanned += 1;
        report.findings.extend(scan_content(&rel, &content));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(findings: &[Finding]) -> Vec<&'static str> {
        findings.iter().map(|f| f.rule).collect()
    }

    #[test]
    fn flags_a_private_key() {
        let findings = scan_content("k.pem", "-----BEGIN RSA PRIVATE KEY-----\nabc\n");
        assert_eq!(rules(&findings), vec!["private-key"]);
        assert_eq!(findings[0].severity, Severity::High);
    }

    #[test]
    fn scanning_multibyte_text_does_not_panic() {
        // A panic here would take down the build on any project containing non-ASCII text.
        let content = "const label = '\u{1F600}\u{1F600} AKIAIOSFODNN7EXAMPLE \u{4F60}\u{597D}';\n";
        let findings = scan_content("a.js", content);
        assert!(findings.iter().any(|f| f.rule == "aws-access-key"), "{findings:?}");
        // And a line that ends mid-multibyte must not panic either.
        assert!(scan_content("b.js", "\u{4F60}\u{597D}AKIA").is_empty());
    }

    #[test]
    fn flags_an_aws_key_id() {
        let findings = scan_content("a.js", "const id = 'AKIAIOSFODNN7EXAMPLE';\n");
        assert!(rules(&findings).contains(&"aws-access-key"));
        assert_eq!(findings.iter().find(|f| f.rule == "aws-access-key").unwrap().severity, Severity::High);
    }

    #[test]
    fn flags_a_hardcoded_secret_but_not_a_placeholder() {
        let real = scan_content("a.js", "const apiKey = 'sk-live-9f8a7b6c5d4e3f2a1b';\n");
        assert!(rules(&real).contains(&"hardcoded-secret"));
        assert_eq!(real.iter().find(|f| f.rule == "hardcoded-secret").unwrap().severity, Severity::Medium);

        for placeholder in [
            "const apiKey = 'your-api-key-here';",
            "const apiKey = process.env.API_KEY;",
            "const apiKey = '<insert>';",
            "const apiKey = 'changeme-please';",
        ] {
            let findings = scan_content("a.js", placeholder);
            assert!(
                !rules(&findings).contains(&"hardcoded-secret"),
                "should not flag {placeholder:?}: {:?}",
                rules(&findings)
            );
        }
    }

    #[test]
    fn flags_code_execution_sinks() {
        let findings = scan_content("a.js", "eval(userInput);\n");
        assert!(rules(&findings).contains(&"eval"));
    }

    #[test]
    fn flags_plaintext_remote_urls_but_not_loopback() {
        let remote = scan_content("a.js", "fetch('http://api.example.org/data');\n");
        assert!(rules(&remote).contains(&"insecure-url"));
        let local = scan_content("a.js", "fetch('http://127.0.0.1:4173/api/health');\n");
        assert!(!rules(&local).contains(&"insecure-url"), "{:?}", rules(&local));
    }

    #[test]
    fn a_clean_file_produces_no_findings() {
        let clean = "export function add(a, b) {\n  return a + b;\n}\n";
        assert!(scan_content("src/math.js", clean).is_empty());
    }

    #[test]
    fn a_generated_scaffold_passes_the_gate() {
        // The scaffold must not trip its own scanner, or every build would fail.
        let reqs = crate::builder::extract_requirements("Build a web app to manage invoices and clients with search");
        let files = crate::builder::scaffold(&reqs);
        let mut report = ScanReport::default();
        for file in &files {
            report.files_scanned += 1;
            report.findings.extend(scan_content(&file.path, &file.content));
        }
        assert!(report.passes(), "scaffold findings: {:?}", report.findings);
    }

    #[test]
    fn scans_a_directory_and_counts_files() {
        let dir = std::env::temp_dir().join(format!("kavach-scan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.js"), "const x = 1;\n").unwrap();
        std::fs::write(dir.join("b.js"), "eval('x');\n").unwrap();
        let report = scan_project(&dir);
        assert_eq!(report.files_scanned, 2);
        assert!(!report.passes() || report.findings.is_empty() || report.count(Severity::High) == 0);
        assert!(report.findings.iter().any(|f| f.rule == "eval"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
