use std::path::Path;

/// Maps a file extension to a language identifier.
/// Returns `None` for unsupported languages (content-only indexing, no symbols).
pub fn detect_language(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()? {
        "rs" => Some("rust"),
        "py" | "pyi" => Some("python"),
        "go" => Some("go"),
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        "ts" | "tsx" | "mts" | "cts" => Some("typescript"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn known_extensions() {
        assert_eq!(detect_language(Path::new("foo.rs")), Some("rust"));
        assert_eq!(detect_language(Path::new("bar.py")), Some("python"));
        assert_eq!(detect_language(Path::new("baz.go")), Some("go"));
        assert_eq!(detect_language(Path::new("qux.js")), Some("javascript"));
        assert_eq!(detect_language(Path::new("qux.ts")), Some("typescript"));
        assert_eq!(detect_language(Path::new("qux.tsx")), Some("typescript"));
        assert_eq!(detect_language(Path::new("qux.jsx")), Some("javascript"));
    }

    #[test]
    fn unknown_extensions() {
        assert_eq!(detect_language(Path::new("readme.md")), None);
        assert_eq!(detect_language(Path::new("config.json")), None);
        assert_eq!(detect_language(Path::new("Makefile")), None);
        assert_eq!(detect_language(Path::new(".gitignore")), None);
    }

    #[test]
    fn no_extension() {
        assert_eq!(detect_language(&PathBuf::from("Makefile")), None);
        assert_eq!(detect_language(&PathBuf::from("LICENSE")), None);
    }
}
