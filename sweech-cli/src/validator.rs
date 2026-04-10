use crate::manifest::{BuildMode, DeployTarget, Manifest, ServeMode};
use crate::scanner::ScannedProject;

// ─── What is this file? ───────────────────────────────────────────────────────
//
// The pre-flight validator runs before every build (and on `sweech check`).
// It catches configuration errors that would produce broken deployments
// BEFORE any compilation happens.
//
// Each rule is a separate function that returns Vec<ValidationError>.
// The top-level `validate()` runs all rules and collects all errors.
// We report ALL errors at once — not just the first one — so developers
// can fix everything in one pass.
//
// Rules are separated into:
//   - Errors: hard stops — build cannot proceed
//   - Warnings: proceed with caution — something suspicious but not fatal

#[derive(Debug, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
}

impl ValidationIssue {
    fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: message.into(),
        }
    }

    fn warning(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code,
            message: message.into(),
        }
    }
}

/// Run all validation rules. Returns all issues found (errors + warnings).
/// Caller decides whether to abort based on whether any Errors are present.
pub fn validate(manifest: &Manifest, project: &ScannedProject) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    issues.extend(check_monolith_single_runtime(manifest));
    issues.extend(check_no_secrets_in_manifest(manifest));
    issues.extend(check_applet_paths_exist(manifest, project));
    issues.extend(check_frontend_serve_rules(manifest));
    issues.extend(check_serverless_frontend_deploy_target(manifest));
    issues.extend(check_applet_runtime_in_monolith(manifest));

    issues
}

/// True if the result of validate() has any hard errors.
pub fn has_errors(issues: &[ValidationIssue]) -> bool {
    issues.iter().any(|i| i.severity == Severity::Error)
}

// ─── Rule implementations ─────────────────────────────────────────────────────

/// MONOLITH_SINGLE_RUNTIME: In monolith mode, no applet may declare
/// a different runtime than [build].runtime.
fn check_monolith_single_runtime(manifest: &Manifest) -> Vec<ValidationIssue> {
    if manifest.build.mode != BuildMode::Monolith {
        return vec![];
    }

    manifest
        .applets
        .iter()
        .filter_map(|applet| match &applet.runtime {
            Some(rt) if rt != &manifest.build.runtime => Some(ValidationIssue::error(
                "MONOLITH_MIXED_RUNTIME",
                format!(
                    "Applet '{}' declares runtime '{:?}' but monolith build runtime is '{:?}'. \
                             In monolith mode all applets must use the same runtime, \
                             declared at [build] level.",
                    applet.name, rt, manifest.build.runtime,
                ),
            )),
            _ => None,
        })
        .collect()
}

/// NO_SECRETS: The manifest must never contain secret values.
/// We detect obvious patterns — actual secret management is out of scope.
fn check_no_secrets_in_manifest(manifest: &Manifest) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    // Check plugin configs for hardcoded connection strings
    // (Future: scan all string values in the manifest for secret patterns)
    // For now: warn if any task `run` command contains obvious secret patterns
    for (name, task) in &manifest.tasks {
        if let Some(run) = &task.run {
            if run.contains("SECRET") || run.contains("PASSWORD") || run.contains("TOKEN") {
                issues.push(ValidationIssue::error(
                    "SECRET_IN_MANIFEST",
                    format!(
                        "Task '{}' run command may contain a secret. \
                         Use environment variables instead of hardcoding secrets.",
                        name
                    ),
                ));
            }
        }
    }

    issues
}

/// APPLET_PATH_EXISTS: Every [[applet]] path in the manifest must
/// correspond to a directory found by the scanner.
fn check_applet_paths_exist(manifest: &Manifest, project: &ScannedProject) -> Vec<ValidationIssue> {
    manifest
        .applets
        .iter()
        .filter_map(|applet_manifest| {
            let found = project
                .applets
                .iter()
                .any(|a| a.name == applet_manifest.name);
            if !found {
                Some(ValidationIssue::error(
                    "APPLET_NOT_FOUND",
                    format!(
                        "Applet '{}' is declared in the manifest (path: '{}') \
                         but no corresponding .applet directory was found.",
                        applet_manifest.name, applet_manifest.path,
                    ),
                ))
            } else {
                None
            }
        })
        .collect()
}

/// FRONTEND_SERVE_RULES:
///   - serve = "embedded" is a hard error on Expo or Ionic
///   - serve is required when deploy_target = "^build" on a web frontend
///   - api_prefix on standalone frontend generates a warning (it's ignored)
fn check_frontend_serve_rules(manifest: &Manifest) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    for frontend in &manifest.frontends {
        let is_mobile = frontend.framework.is_mobile();

        // embedded on mobile is a hard error
        if is_mobile && frontend.serve == Some(ServeMode::Embedded) {
            issues.push(ValidationIssue::error(
                "EMBEDDED_ON_MOBILE",
                format!(
                    "Frontend '{}' uses framework '{:?}' (mobile) with serve = 'embedded'. \
                     Embedded mode is only valid for web frameworks.",
                    frontend.name, frontend.framework
                ),
            ));
        }

        // ^build web frontend without serve is a hard error
        if !is_mobile && frontend.deploy_target == DeployTarget::Build && frontend.serve.is_none() {
            issues.push(ValidationIssue::error(
                "MISSING_SERVE_ON_BUILD_FRONTEND",
                format!(
                    "Frontend '{}' has deploy_target = '^build' but no serve mode declared. \
                     Add `serve = \"embedded\"` or `serve = \"standalone\"`.",
                    frontend.name
                ),
            ));
        }

        if frontend.serve == Some(ServeMode::Standalone) && frontend.api_prefix != "/api" {
            issues.push(ValidationIssue::warning(
                "API_PREFIX_ON_STANDALONE",
                format!(
                    "Frontend '{}' is standalone but declares api_prefix = '{}'. \
                     api_prefix is only used in embedded mode — it will be ignored.",
                    frontend.name, frontend.api_prefix
                ),
            ));
        }
    }

    issues
}

fn check_serverless_frontend_deploy_target(manifest: &Manifest) -> Vec<ValidationIssue> {
    if manifest.build.mode != BuildMode::Serverless {
        return vec![];
    }
    manifest
        .frontends
        .iter()
        .filter_map(|frontend| {
            if frontend.deploy_target == DeployTarget::Build {
                Some(ValidationIssue::error(
                    "SERVERLESS_GENERIC_DEPLOY_TARGET",
                    format!(
                        "Frontend '{}' uses deploy_target = '^build' in serverless mode. \
                         Use an explicit target such as deploy_target = 'vercel'.",
                        frontend.name
                    ),
                ))
            } else {
                None
            }
        })
        .collect()
}

/// APPLET_RUNTIME_IN_MONOLITH: Applets may not declare a runtime in monolith
/// mode (even if it matches — the declaration itself is wrong).
/// Warn rather than error, since it's unambiguous what to do.
fn check_applet_runtime_in_monolith(manifest: &Manifest) -> Vec<ValidationIssue> {
    if manifest.build.mode != BuildMode::Monolith {
        return vec![];
    }

    manifest
        .applets
        .iter()
        .filter_map(|applet| {
            if applet.runtime.is_some() {
                Some(ValidationIssue::warning(
                    "APPLET_RUNTIME_IN_MONOLITH",
                    format!(
                        "Applet '{}' declares a runtime but mode is monolith. \
                         Per-applet runtime is only valid in microservices/serverless mode. \
                         Remove the runtime declaration or change the build mode.",
                        applet.name
                    ),
                ))
            } else {
                None
            }
        })
        .collect()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::*;
    use crate::scanner::ScannedProject;
    use std::path::PathBuf;

    fn empty_project() -> ScannedProject {
        ScannedProject {
            root: PathBuf::from("/tmp"),
            applets: vec![],
        }
    }

    fn base_manifest(mode: BuildMode) -> Manifest {
        toml::from_str(&format!(
            r#"[project]
name = "test"
[build]
mode = "{}"
"#,
            match mode {
                BuildMode::Monolith => "monolith",
                BuildMode::Microservices => "microservices",
                BuildMode::Serverless => "serverless",
            }
        ))
        .unwrap()
    }

    #[test]
    fn clean_manifest_has_no_issues() {
        let manifest = base_manifest(BuildMode::Monolith);
        let issues = validate(&manifest, &empty_project());
        assert!(issues.is_empty(), "{:?}", issues);
    }

    #[test]
    fn monolith_with_mixed_runtime_is_error() {
        let manifest: Manifest = toml::from_str(
            r#"[project]
name = "test"
[build]
mode = "monolith"
runtime = "rust"
[[applet]]
name = "auth"
path = "auth.applet"
runtime = "typescript"
"#,
        )
        .unwrap();
        let issues = validate(&manifest, &empty_project());
        assert!(has_errors(&issues));
        assert!(issues.iter().any(|i| i.code == "MONOLITH_MIXED_RUNTIME"));
    }

    #[test]
    fn embedded_on_expo_is_error() {
        let manifest: Manifest = toml::from_str(
            r#"[project]
name = "test"
[build]
mode = "monolith"
[[frontend]]
name = "mobile"
path = "apps/mobile"
framework = "expo"
deploy_target = "eas"
serve = "embedded"
"#,
        )
        .unwrap();
        let issues = validate(&manifest, &empty_project());
        assert!(has_errors(&issues));
        assert!(issues.iter().any(|i| i.code == "EMBEDDED_ON_MOBILE"));
    }

    #[test]
    fn build_frontend_missing_serve_is_error() {
        let manifest: Manifest = toml::from_str(
            r#"[project]
name = "test"
[build]
mode = "monolith"
[[frontend]]
name = "web"
path = "apps/web"
framework = "next"
deploy_target = "^build"
"#,
        )
        .unwrap();
        let issues = validate(&manifest, &empty_project());
        assert!(has_errors(&issues));
        assert!(
            issues
                .iter()
                .any(|i| i.code == "MISSING_SERVE_ON_BUILD_FRONTEND")
        );
    }

    #[test]
    fn serverless_with_generic_deploy_target_is_error() {
        let manifest: Manifest = toml::from_str(
            r#"[project]
name = "test"
[build]
mode = "serverless"
[[frontend]]
name = "web"
path = "apps/web"
framework = "next"
deploy_target = "^build"
serve = "standalone"
"#,
        )
        .unwrap();
        let issues = validate(&manifest, &empty_project());
        assert!(has_errors(&issues));
        assert!(
            issues
                .iter()
                .any(|i| i.code == "SERVERLESS_GENERIC_DEPLOY_TARGET")
        );
    }

    #[test]
    fn applet_runtime_in_monolith_is_warning() {
        let manifest: Manifest = toml::from_str(
            r#"[project]
name = "test"
[build]
mode = "monolith"
[[applet]]
name = "auth"
path = "auth.applet"
runtime = "rust"
"#,
        )
        .unwrap();
        // Include the applet in the scanned project so APPLET_NOT_FOUND doesn't fire
        let project = ScannedProject {
            root: PathBuf::from("/tmp"),
            applets: vec![crate::scanner::ScannedApplet {
                name: "auth".to_string(),
                path: PathBuf::from("/tmp/auth.applet"),
                routes: vec![],
            }],
        };
        let issues = validate(&manifest, &project);
        assert!(!has_errors(&issues), "unexpected errors: {:?}", issues);
        assert!(
            issues
                .iter()
                .any(|i| i.code == "APPLET_RUNTIME_IN_MONOLITH")
        );
    }
}
