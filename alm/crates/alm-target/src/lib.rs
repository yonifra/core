//! ALM Target Registry — platform detection, target triple mapping, output formats.
//!
//! Maps ALM target names (e.g., "linux-x86_64") to LLVM target triples
//! and output format metadata.

use std::fmt;

/// All supported ALM targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    LinuxX86_64,
    LinuxAarch64,
    DarwinArm64,
    DarwinX86_64,
    WindowsX86_64,
    Wasm32,
}

impl Target {
    /// All known targets.
    pub const ALL: &[Target] = &[
        Target::LinuxX86_64,
        Target::LinuxAarch64,
        Target::DarwinArm64,
        Target::DarwinX86_64,
        Target::WindowsX86_64,
        Target::Wasm32,
    ];

    /// Parse ALM target name string.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "linux-x86_64" | "linux-x86-64" | "x86_64-linux" => Some(Target::LinuxX86_64),
            "linux-aarch64" | "linux-arm64" | "aarch64-linux" => Some(Target::LinuxAarch64),
            "darwin-arm64" | "darwin-aarch64" | "macos-arm64" => Some(Target::DarwinArm64),
            "darwin-x86_64" | "macos-x86_64" | "macos-intel" => Some(Target::DarwinX86_64),
            "windows-x86_64" | "win-x86_64" | "win64" => Some(Target::WindowsX86_64),
            "wasm32" | "wasm" => Some(Target::Wasm32),
            _ => None,
        }
    }

    /// Canonical ALM target name.
    pub fn name(&self) -> &'static str {
        match self {
            Target::LinuxX86_64 => "linux-x86_64",
            Target::LinuxAarch64 => "linux-aarch64",
            Target::DarwinArm64 => "darwin-arm64",
            Target::DarwinX86_64 => "darwin-x86_64",
            Target::WindowsX86_64 => "windows-x86_64",
            Target::Wasm32 => "wasm32",
        }
    }

    /// LLVM target triple.
    pub fn llvm_triple(&self) -> &'static str {
        match self {
            Target::LinuxX86_64 => "x86_64-unknown-linux-gnu",
            Target::LinuxAarch64 => "aarch64-unknown-linux-gnu",
            Target::DarwinArm64 => "aarch64-apple-darwin",
            Target::DarwinX86_64 => "x86_64-apple-darwin",
            Target::WindowsX86_64 => "x86_64-pc-windows-msvc",
            Target::Wasm32 => "wasm32-unknown-unknown",
        }
    }

    /// Output binary format.
    pub fn object_format(&self) -> ObjectFormat {
        match self {
            Target::LinuxX86_64 | Target::LinuxAarch64 => ObjectFormat::Elf,
            Target::DarwinArm64 | Target::DarwinX86_64 => ObjectFormat::MachO,
            Target::WindowsX86_64 => ObjectFormat::PeCoff,
            Target::Wasm32 => ObjectFormat::Wasm,
        }
    }

    /// Default output file extension.
    pub fn exe_extension(&self) -> &'static str {
        match self {
            Target::WindowsX86_64 => ".exe",
            Target::Wasm32 => ".wasm",
            _ => "",
        }
    }

    /// Object file extension.
    pub fn obj_extension(&self) -> &'static str {
        match self {
            Target::WindowsX86_64 => ".obj",
            Target::Wasm32 => ".wasm",
            _ => ".o",
        }
    }

    /// OS name.
    pub fn os(&self) -> &'static str {
        match self {
            Target::LinuxX86_64 | Target::LinuxAarch64 => "linux",
            Target::DarwinArm64 | Target::DarwinX86_64 => "darwin",
            Target::WindowsX86_64 => "windows",
            Target::Wasm32 => "wasm",
        }
    }

    /// Architecture name.
    pub fn arch(&self) -> &'static str {
        match self {
            Target::LinuxX86_64 | Target::DarwinX86_64 | Target::WindowsX86_64 => "x86_64",
            Target::LinuxAarch64 | Target::DarwinArm64 => "aarch64",
            Target::Wasm32 => "wasm32",
        }
    }

    /// Whether this target uses WASM backend (not LLVM native).
    pub fn is_wasm(&self) -> bool {
        matches!(self, Target::Wasm32)
    }

    /// Whether this target can be natively linked on the current host.
    pub fn can_link_on_host(&self) -> bool {
        let host = Self::host();
        match self {
            // Same-OS linking works (macOS can link both arm64 and x86_64)
            t if t.os() == host.os() => true,
            // WASM doesn't need a system linker
            Target::Wasm32 => true,
            // Cross-OS needs a cross-linker (not guaranteed)
            _ => false,
        }
    }

    /// Detect current host platform.
    pub fn host() -> Self {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        { Target::LinuxX86_64 }
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        { Target::LinuxAarch64 }
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        { Target::DarwinArm64 }
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        { Target::DarwinX86_64 }
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        { Target::WindowsX86_64 }
        #[cfg(target_arch = "wasm32")]
        { Target::Wasm32 }
    }

    /// Output subdirectory name for multi-target builds.
    pub fn output_dir(&self) -> String {
        format!("{}-{}", self.os(), self.arch())
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Binary object format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectFormat {
    Elf,
    MachO,
    PeCoff,
    Wasm,
}

impl fmt::Display for ObjectFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ObjectFormat::Elf => write!(f, "ELF"),
            ObjectFormat::MachO => write!(f, "Mach-O"),
            ObjectFormat::PeCoff => write!(f, "PE/COFF"),
            ObjectFormat::Wasm => write!(f, "WASM"),
        }
    }
}

/// Build output specification for a target.
#[derive(Debug, Clone)]
pub struct BuildSpec {
    pub target: Target,
    pub output_path: String,
    pub obj_path: String,
}

impl BuildSpec {
    pub fn new(target: Target, base_name: &str, output_dir: Option<&str>) -> Self {
        let dir = output_dir
            .map(|d| format!("{}/{}", d, target.output_dir()))
            .unwrap_or_default();

        let output_path = if dir.is_empty() {
            format!("{}{}", base_name, target.exe_extension())
        } else {
            format!("{}/{}{}", dir, base_name, target.exe_extension())
        };

        let obj_path = if dir.is_empty() {
            format!("{}{}", base_name, target.obj_extension())
        } else {
            format!("{}/{}{}", dir, base_name, target.obj_extension())
        };

        BuildSpec { target, output_path, obj_path }
    }
}

/// Resolve target list from alm.yaml config target names.
pub fn resolve_targets(names: &[String]) -> Result<Vec<Target>, String> {
    let mut targets = Vec::new();
    for name in names {
        match Target::from_name(name) {
            Some(t) => targets.push(t),
            None => return Err(format!("unknown target: '{name}'. Valid: {}",
                Target::ALL.iter().map(|t| t.name()).collect::<Vec<_>>().join(", "))),
        }
    }
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_all_targets() {
        for t in Target::ALL {
            let parsed = Target::from_name(t.name());
            assert_eq!(parsed, Some(*t), "failed to parse {}", t.name());
        }
    }

    #[test]
    fn test_aliases() {
        assert_eq!(Target::from_name("macos-arm64"), Some(Target::DarwinArm64));
        assert_eq!(Target::from_name("win64"), Some(Target::WindowsX86_64));
        assert_eq!(Target::from_name("wasm"), Some(Target::Wasm32));
        assert_eq!(Target::from_name("linux-arm64"), Some(Target::LinuxAarch64));
    }

    #[test]
    fn test_unknown_target() {
        assert_eq!(Target::from_name("riscv-64"), None);
    }

    #[test]
    fn test_llvm_triples() {
        assert_eq!(Target::LinuxX86_64.llvm_triple(), "x86_64-unknown-linux-gnu");
        assert_eq!(Target::DarwinArm64.llvm_triple(), "aarch64-apple-darwin");
        assert_eq!(Target::WindowsX86_64.llvm_triple(), "x86_64-pc-windows-msvc");
    }

    #[test]
    fn test_object_formats() {
        assert_eq!(Target::LinuxX86_64.object_format(), ObjectFormat::Elf);
        assert_eq!(Target::DarwinArm64.object_format(), ObjectFormat::MachO);
        assert_eq!(Target::WindowsX86_64.object_format(), ObjectFormat::PeCoff);
        assert_eq!(Target::Wasm32.object_format(), ObjectFormat::Wasm);
    }

    #[test]
    fn test_exe_extensions() {
        assert_eq!(Target::LinuxX86_64.exe_extension(), "");
        assert_eq!(Target::WindowsX86_64.exe_extension(), ".exe");
        assert_eq!(Target::Wasm32.exe_extension(), ".wasm");
    }

    #[test]
    fn test_host_detection() {
        let host = Target::host();
        // We're running on some platform — should be valid
        assert!(Target::ALL.contains(&host));
    }

    #[test]
    fn test_wasm_is_wasm() {
        assert!(Target::Wasm32.is_wasm());
        assert!(!Target::LinuxX86_64.is_wasm());
    }

    #[test]
    fn test_can_link_host() {
        let host = Target::host();
        assert!(host.can_link_on_host());
        assert!(Target::Wasm32.can_link_on_host()); // WASM always linkable
    }

    #[test]
    fn test_build_spec() {
        let spec = BuildSpec::new(Target::LinuxX86_64, "app", Some("build"));
        assert_eq!(spec.output_path, "build/linux-x86_64/app");
        assert_eq!(spec.obj_path, "build/linux-x86_64/app.o");
    }

    #[test]
    fn test_build_spec_windows() {
        let spec = BuildSpec::new(Target::WindowsX86_64, "app", Some("build"));
        assert_eq!(spec.output_path, "build/windows-x86_64/app.exe");
        assert_eq!(spec.obj_path, "build/windows-x86_64/app.obj");
    }

    #[test]
    fn test_build_spec_wasm() {
        let spec = BuildSpec::new(Target::Wasm32, "app", None);
        assert_eq!(spec.output_path, "app.wasm");
    }

    #[test]
    fn test_resolve_targets() {
        let names = vec!["linux-x86_64".into(), "darwin-arm64".into(), "wasm32".into()];
        let targets = resolve_targets(&names).unwrap();
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0], Target::LinuxX86_64);
    }

    #[test]
    fn test_resolve_targets_error() {
        let names = vec!["bogus-target".into()];
        assert!(resolve_targets(&names).is_err());
    }

    #[test]
    fn test_output_dir() {
        assert_eq!(Target::LinuxX86_64.output_dir(), "linux-x86_64");
        assert_eq!(Target::DarwinArm64.output_dir(), "darwin-aarch64");
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Target::LinuxX86_64), "linux-x86_64");
        assert_eq!(format!("{}", ObjectFormat::Elf), "ELF");
    }
}
