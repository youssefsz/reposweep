use std::fs;
use std::path::{Path, PathBuf};

use crate::config::{Config, CustomRuleConfig, resolve_protected_path};
use crate::domain::ArtifactKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleMatchKind {
    Exact,
    GenericWithMarkers,
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub name: String,
    pub directory_name: String,
    pub kind: ArtifactKind,
    pub ecosystem: String,
    pub required_markers: Vec<String>,
    pub enabled: bool,
    pub match_kind: RuleMatchKind,
}

#[derive(Clone, Debug)]
pub struct RuleSet {
    rules: Vec<Rule>,
    protected_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct RuleMatch {
    pub rule: Rule,
    pub project_root: Option<PathBuf>,
}

impl RuleSet {
    pub fn from_config(config: &Config) -> Self {
        let disabled: Vec<String> = config
            .disabled_ecosystems
            .iter()
            .map(|item| item.to_lowercase())
            .collect();

        let mut rules: Vec<Rule> = built_in_rules()
            .into_iter()
            .filter(|rule| !disabled.contains(&rule.ecosystem.to_lowercase()) && rule.enabled)
            .collect();

        rules.extend(config.custom_rules.iter().map(custom_rule));

        Self {
            rules,
            protected_paths: config.protected_paths.clone(),
        }
    }

    pub fn match_directory(&self, path: &Path, root: &Path) -> Option<RuleMatch> {
        let name = path.file_name()?.to_string_lossy();
        self.rules.iter().find_map(|rule| {
            if rule.directory_name != name {
                return None;
            }

            if rule.required_markers.is_empty() {
                return Some(RuleMatch {
                    rule: rule.clone(),
                    project_root: path.parent().map(Path::to_path_buf),
                });
            }

            find_marker_ancestor(path, root, &rule.required_markers).map(|project_root| RuleMatch {
                rule: rule.clone(),
                project_root: Some(project_root),
            })
        })
    }

    pub fn is_protected_path(&self, root: &Path, path: &Path) -> bool {
        self.protected_paths.iter().any(|protected| {
            let protected = resolve_protected_path(root, protected);
            path == protected || path.starts_with(protected)
        })
    }
}

pub fn built_in_rules() -> Vec<Rule> {
    vec![
        exact(
            "Node Modules",
            "node_modules",
            ArtifactKind::Dependency,
            "javascript",
        ),
        exact(
            ".pnpm-store",
            ".pnpm-store",
            ArtifactKind::Cache,
            "javascript",
        ),
        exact(".yarn", ".yarn", ArtifactKind::Cache, "javascript"),
        exact(".next", ".next", ArtifactKind::Build, "javascript"),
        exact(".turbo", ".turbo", ArtifactKind::Cache, "javascript"),
        exact(".cache", ".cache", ArtifactKind::Cache, "generic"),
        exact(
            ".parcel-cache",
            ".parcel-cache",
            ArtifactKind::Cache,
            "javascript",
        ),
        exact("__pycache__", "__pycache__", ArtifactKind::Cache, "python"),
        exact(
            ".pytest_cache",
            ".pytest_cache",
            ArtifactKind::Cache,
            "python",
        ),
        exact(".mypy_cache", ".mypy_cache", ArtifactKind::Cache, "python"),
        exact(".ruff_cache", ".ruff_cache", ArtifactKind::Cache, "python"),
        exact(".venv", ".venv", ArtifactKind::Dependency, "python"),
        exact("venv", "venv", ArtifactKind::Dependency, "python"),
        exact(".tox", ".tox", ArtifactKind::Build, "python"),
        exact("target", "target", ArtifactKind::Build, "rust"),
        exact(".gradle", ".gradle", ArtifactKind::Cache, "java"),
        exact(".dart_tool", ".dart_tool", ArtifactKind::Cache, "dart"),
        exact(".pub-cache", ".pub-cache", ArtifactKind::Dependency, "dart"),
        exact(".idea", ".idea", ArtifactKind::Cache, "generic"),
        generic(
            "Build Output",
            "build",
            ArtifactKind::Build,
            "generic",
            &[
                "package.json",
                "Cargo.toml",
                "pyproject.toml",
                "requirements.txt",
                "build.gradle",
                "build.gradle.kts",
                "settings.gradle",
                "pubspec.yaml",
                "go.mod",
            ],
        ),
        generic(
            "Distribution Output",
            "dist",
            ArtifactKind::Build,
            "javascript",
            &["package.json"],
        ),
        generic(
            ".NET bin",
            "bin",
            ArtifactKind::Build,
            "dotnet",
            &["*.csproj", "*.fsproj", "*.vbproj", "*.sln"],
        ),
        generic(
            ".NET obj",
            "obj",
            ArtifactKind::Build,
            "dotnet",
            &["*.csproj", "*.fsproj", "*.vbproj", "*.sln"],
        ),
    ]
}

fn exact(name: &str, dir: &str, kind: ArtifactKind, ecosystem: &str) -> Rule {
    Rule {
        name: name.into(),
        directory_name: dir.into(),
        kind,
        ecosystem: ecosystem.into(),
        required_markers: vec![],
        enabled: true,
        match_kind: RuleMatchKind::Exact,
    }
}

fn generic(
    name: &str,
    dir: &str,
    kind: ArtifactKind,
    ecosystem: &str,
    required_markers: &[&str],
) -> Rule {
    Rule {
        name: name.into(),
        directory_name: dir.into(),
        kind,
        ecosystem: ecosystem.into(),
        required_markers: required_markers.iter().map(|item| (*item).into()).collect(),
        enabled: true,
        match_kind: RuleMatchKind::GenericWithMarkers,
    }
}

fn custom_rule(config: &CustomRuleConfig) -> Rule {
    Rule {
        name: config.name.clone(),
        directory_name: config.directory_name.clone(),
        kind: config.kind,
        ecosystem: config.ecosystem.clone(),
        required_markers: config.required_markers.clone(),
        enabled: config.enabled,
        match_kind: if config.required_markers.is_empty() {
            RuleMatchKind::Exact
        } else {
            RuleMatchKind::GenericWithMarkers
        },
    }
}

fn find_marker_ancestor(path: &Path, root: &Path, markers: &[String]) -> Option<PathBuf> {
    let mut current = path.parent();
    while let Some(dir) = current {
        if dir == root || dir.starts_with(root) {
            if markers.iter().any(|marker| marker_exists(dir, marker)) {
                return Some(dir.to_path_buf());
            }
        }

        if dir == root {
            break;
        }
        current = dir.parent();
    }
    None
}

fn marker_exists(dir: &Path, marker: &str) -> bool {
    if let Some((_, ext)) = marker.split_once("*.") {
        return fs::read_dir(dir)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(std::result::Result::ok))
            .filter(|entry| entry.path().is_file())
            .any(|entry| entry.path().extension().is_some_and(|value| value == ext));
    }

    dir.join(marker).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_rule_requires_markers() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let build = root.join("build");
        std::fs::create_dir_all(&build).expect("create build");

        let rules = RuleSet::from_config(&Config::default());
        assert!(rules.match_directory(&build, root).is_none());

        std::fs::write(root.join("package.json"), "{}").expect("write marker");
        assert!(rules.match_directory(&build, root).is_some());
    }
}
