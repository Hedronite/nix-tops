//! Which Nix the host runs, and whether it meets the daemon floor (protocol section 2):
//! CppNix 2.24 or later, or Determinate Nix 3.x.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Implementation {
    CppNix,
    Determinate,
    Lix,
    Unknown,
}

impl Implementation {
    pub fn name(self) -> &'static str {
        match self {
            Implementation::CppNix => "CppNix",
            Implementation::Determinate => "Determinate Nix",
            Implementation::Lix => "Lix",
            Implementation::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostNix {
    pub implementation: Implementation,
    /// The implementation's own version: Determinate's 3.x, or the Nix version for the others.
    pub version: String,
    /// The Nix version the binary reports (the trailing number of `nix --version`).
    pub nix_version: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Floor {
    Met,
    Below(String),
}

fn classify(inside: &str, nix_version: &str) -> (Implementation, String) {
    if inside == "Nix" {
        (Implementation::CppNix, nix_version.to_string())
    } else if let Some(version) = inside.strip_prefix("Determinate Nix ") {
        (Implementation::Determinate, version.to_string())
    } else if inside.starts_with("Lix") {
        (Implementation::Lix, nix_version.to_string())
    } else {
        (Implementation::Unknown, nix_version.to_string())
    }
}

/// Parses the first line of `nix --version`: `nix (Nix) 2.35.2`,
/// `nix (Determinate Nix 3.22.3) 2.35.2`, `nix (Lix, like Nix) 2.95.0`.
pub fn parse_version_line(line: &str) -> Option<HostNix> {
    let rest = line.trim().strip_prefix("nix (")?;
    let (inside, nix_version) = rest.split_once(") ")?;
    let (implementation, version) = classify(inside, nix_version.trim());
    Some(HostNix {
        implementation,
        version,
        nix_version: nix_version.trim().to_string(),
    })
}

/// Major and minor of `2.24.1` or `2.24pre20240101_abc`.
pub fn major_minor(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor_text = parts.next()?;
    let digits: String = minor_text
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    Some((major, digits.parse().ok()?))
}

fn cppnix_floor(version: &str) -> Floor {
    match major_minor(version) {
        Some(v) if v >= (2, 24) => Floor::Met,
        _ => Floor::Below(format!("CppNix {version} is below 2.24")),
    }
}

fn determinate_floor(version: &str) -> Floor {
    match major_minor(version) {
        Some((major, _)) if major >= 3 => Floor::Met,
        _ => Floor::Below(format!("Determinate Nix {version} is below 3.x")),
    }
}

/// The daemon floor. Lix is not supported until the implementation matrix.
pub fn check_floor(host: &HostNix) -> Floor {
    match host.implementation {
        Implementation::CppNix => cppnix_floor(&host.version),
        Implementation::Determinate => determinate_floor(&host.version),
        Implementation::Lix => Floor::Below("Lix is not supported yet".to_string()),
        Implementation::Unknown => Floor::Below("an unrecognised Nix implementation".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(line: &str) -> HostNix {
        parse_version_line(line).expect(line)
    }

    #[test]
    fn version_lines_parse() {
        let cpp = host("nix (Nix) 2.35.2");
        assert_eq!(
            (
                cpp.implementation,
                cpp.version.as_str(),
                cpp.nix_version.as_str()
            ),
            (Implementation::CppNix, "2.35.2", "2.35.2")
        );
        let det = host("nix (Determinate Nix 3.22.3) 2.35.2");
        assert_eq!(
            (
                det.implementation,
                det.version.as_str(),
                det.nix_version.as_str()
            ),
            (Implementation::Determinate, "3.22.3", "2.35.2")
        );
        assert_eq!(
            host("nix (Lix, like Nix) 2.95.0").implementation,
            Implementation::Lix
        );
        assert_eq!(
            host("nix (Other) 1.0.0").implementation,
            Implementation::Unknown
        );
        assert_eq!(parse_version_line("not nix"), None);
        assert_eq!(parse_version_line("nix (Nix)"), None);
    }

    #[test]
    fn major_minor_reads_prerelease_suffixes() {
        assert_eq!(major_minor("2.24.1"), Some((2, 24)));
        assert_eq!(major_minor("2.24pre20240101_abc"), Some((2, 24)));
        assert_eq!(major_minor("2"), None);
        assert_eq!(major_minor("x.y"), None);
        assert_eq!(major_minor("2.pre"), None);
    }

    #[test]
    fn the_floor() {
        let met = |line: &str| check_floor(&host(line)) == Floor::Met;
        assert!(!met("nix (Nix) 2.23.4"));
        assert!(met("nix (Nix) 2.24.0"));
        assert!(met("nix (Nix) 3.0.0"));
        assert!(met("nix (Determinate Nix 3.0.0) 2.24.0"));
        assert!(!met("nix (Determinate Nix 2.99.0) 2.30.0"));
        assert!(!met("nix (Lix, like Nix) 2.95.0"));
        assert!(!met("nix (Other) 9.0.0"));
        assert!(!met("nix (Nix) garbage"));
    }

    #[test]
    fn the_reasons_name_the_version() {
        assert_eq!(
            check_floor(&host("nix (Nix) 2.23.4")),
            Floor::Below("CppNix 2.23.4 is below 2.24".to_string())
        );
        assert_eq!(Implementation::CppNix.name(), "CppNix");
        assert_eq!(Implementation::Determinate.name(), "Determinate Nix");
        assert_eq!(Implementation::Lix.name(), "Lix");
        assert_eq!(Implementation::Unknown.name(), "unknown");
    }
}
