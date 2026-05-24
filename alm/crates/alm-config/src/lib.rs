//! ALM Config — typed parser for `alm.yaml` universal platform config.
//!
//! One file. LLM reads it, knows everything: what to build, how to test,
//! where to deploy, what to monitor. Zero ambiguity.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlmConfig {
    pub version: String,
    pub project: Project,
    #[serde(default)]
    pub modules: Vec<ModuleConfig>,
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub test: TestConfig,
    #[serde(default)]
    pub ci: CiConfig,
    #[serde(default)]
    pub deploy: HashMap<String, DeployTarget>,
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub agent: AgentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    #[serde(default = "default_lang_version")]
    pub lang_version: String,
    #[serde(default)]
    pub targets: Vec<String>,
}

fn default_lang_version() -> String { "0.1-alpha".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleConfig {
    pub path: String,
    #[serde(default = "default_entry")]
    pub entry: String,
    #[serde(default)]
    pub deps: Vec<Dependency>,
}

fn default_entry() -> String { "main.alm".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    #[serde(default)]
    pub ver: String,
    #[serde(default)]
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    #[serde(default = "default_opt_level")]
    pub opt_level: u8,
    #[serde(default)]
    pub lto: bool,
    #[serde(default)]
    pub sanitizers: Vec<String>,
}

fn default_opt_level() -> u8 { 2 }

impl Default for BuildConfig {
    fn default() -> Self {
        Self { opt_level: 2, lto: false, sanitizers: vec![] }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    #[serde(default = "default_strategy")]
    pub strategy: String,
    #[serde(default = "default_coverage_min")]
    pub coverage_min: u8,
    #[serde(default = "default_sandbox")]
    pub sandbox: String,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_parallel")]
    pub parallel: String,
    #[serde(default)]
    pub on_fail: Option<OnFailConfig>,
}

fn default_strategy() -> String { "unit".into() }
fn default_coverage_min() -> u8 { 80 }
fn default_sandbox() -> String { "strict".into() }
fn default_timeout() -> u64 { 5000 }
fn default_parallel() -> String { "auto".into() }

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            strategy: default_strategy(),
            coverage_min: default_coverage_min(),
            sandbox: default_sandbox(),
            timeout_ms: default_timeout(),
            parallel: default_parallel(),
            on_fail: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnFailConfig {
    #[serde(default)]
    pub retry: u8,
    #[serde(default)]
    pub then: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiConfig {
    #[serde(default)]
    pub trigger: Vec<String>,
    #[serde(default)]
    pub stages: Vec<CiStage>,
}

impl Default for CiConfig {
    fn default() -> Self {
        Self { trigger: vec![], stages: vec![] }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiStage {
    pub name: String,
    pub run: String,
    #[serde(default)]
    pub needs: Vec<String>,
    #[serde(default)]
    pub gate: Option<GateConfig>,
    #[serde(default)]
    pub approval: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateConfig {
    #[serde(default)]
    pub regression_threshold: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployTarget {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default)]
    pub registry: Option<String>,
    #[serde(default)]
    pub health_check: Option<String>,
    #[serde(default)]
    pub rollback: Option<String>,
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub canary_percent: Option<u8>,
    #[serde(default)]
    pub canary_duration: Option<String>,
    #[serde(default)]
    pub promotion: Option<String>,
}

fn default_provider() -> String { "binary".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    #[serde(default = "default_metrics_backend")]
    pub backend: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub default_labels: HashMap<String, String>,
    #[serde(default)]
    pub alerts: Vec<AlertConfig>,
}

fn default_metrics_backend() -> String { "otlp".into() }

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            backend: default_metrics_backend(),
            endpoint: None,
            default_labels: HashMap::new(),
            alerts: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    pub name: String,
    pub expr: String,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub notify: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_context_budget")]
    pub context_budget: u64,
    #[serde(default)]
    pub self_heal: bool,
    #[serde(default = "default_review_mode")]
    pub review_mode: String,
}

fn default_model() -> String { "claude-4".into() }
fn default_context_budget() -> u64 { 200000 }
fn default_review_mode() -> String { "diff".into() }

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            context_budget: default_context_budget(),
            self_heal: false,
            review_mode: default_review_mode(),
        }
    }
}

impl AlmConfig {
    /// Load config from a YAML file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Self::parse(&content)
    }

    /// Parse config from YAML string.
    pub fn parse(yaml: &str) -> Result<Self, String> {
        serde_yaml::from_str(yaml).map_err(|e| format!("config parse error: {e}"))
    }

    /// Find config file by searching up from current dir.
    pub fn find_and_load() -> Result<Self, String> {
        let names = ["alm.yaml", "alm.yml"];
        let mut dir = std::env::current_dir()
            .map_err(|e| format!("cannot get cwd: {e}"))?;
        loop {
            for name in &names {
                let path = dir.join(name);
                if path.exists() {
                    return Self::load(&path);
                }
            }
            if !dir.pop() { break; }
        }
        Err("no alm.yaml found".into())
    }

    /// CI stages in topological order (respecting `needs`).
    pub fn ci_stages_ordered(&self) -> Result<Vec<&CiStage>, String> {
        let mut result = Vec::new();
        let mut done: Vec<String> = Vec::new();
        let mut remaining: Vec<&CiStage> = self.ci.stages.iter().collect();
        let max = remaining.len() * remaining.len() + 1;

        for _ in 0..max {
            if remaining.is_empty() { break; }
            let prev_len = remaining.len();
            let mut next = Vec::new();
            for stage in &remaining {
                if stage.needs.iter().all(|n| done.contains(n)) {
                    result.push(*stage);
                    done.push(stage.name.clone());
                } else {
                    next.push(*stage);
                }
            }
            if next.len() == prev_len {
                let stuck: Vec<&str> = next.iter().map(|s| s.name.as_str()).collect();
                return Err(format!("circular CI deps: {stuck:?}"));
            }
            remaining = next;
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_CONFIG: &str = r#"
version: "0.1.0"
project:
  name: "payment-gateway"
  lang_version: "0.1-alpha"
  targets: [linux-x86_64, darwin-arm64, wasm32]
modules:
  - path: src/
    entry: main.alm
    deps:
      - name: http
        ver: "0.1"
      - name: db
        ver: "0.2"
        features: [postgres, pool]
build:
  opt_level: 3
  lto: true
  sanitizers: [address, thread]
test:
  strategy: property
  coverage_min: 85
  sandbox: strict
  timeout_ms: 5000
  parallel: auto
  on_fail:
    retry: 2
    then: block_deploy
ci:
  trigger: [push, pr]
  stages:
    - name: lint
      run: alm lint --strict
    - name: test
      run: alm test --all
      needs: [lint]
    - name: bench
      run: alm bench --compare=main
      needs: [test]
      gate:
        regression_threshold: "5%"
    - name: build
      run: alm build --release
      needs: [bench]
    - name: deploy
      run: alm deploy --target=staging
      needs: [build]
      approval: auto
deploy:
  staging:
    provider: container
    registry: ghcr.io/org/payment-gateway
    health_check: /healthz
    rollback: auto
  production:
    provider: container
    strategy: canary
    canary_percent: 10
    canary_duration: 15m
    promotion: metric_gate
metrics:
  backend: otlp
  endpoint: "https://otel.internal:4317"
  default_labels:
    service: payment-gateway
    env: "${DEPLOY_ENV}"
  alerts:
    - name: high_error_rate
      expr: "rate(req_errors_total[5m]) > 0.05"
      severity: critical
      notify: [pagerduty, slack]
agent:
  model: claude-4
  context_budget: 200000
  self_heal: true
  review_mode: diff
"#;

    #[test]
    fn test_parse_full_config() {
        let config = AlmConfig::parse(FULL_CONFIG).unwrap();
        assert_eq!(config.project.name, "payment-gateway");
        assert_eq!(config.project.targets.len(), 3);
        assert_eq!(config.build.opt_level, 3);
        assert!(config.build.lto);
        assert_eq!(config.test.strategy, "property");
        assert_eq!(config.test.coverage_min, 85);
        assert_eq!(config.ci.stages.len(), 5);
        assert_eq!(config.deploy.len(), 2);
        assert_eq!(config.metrics.alerts.len(), 1);
        assert!(config.agent.self_heal);
    }

    #[test]
    fn test_minimal_config() {
        let yaml = "version: \"0.1.0\"\nproject:\n  name: simple\n";
        let config = AlmConfig::parse(yaml).unwrap();
        assert_eq!(config.project.name, "simple");
        assert_eq!(config.build.opt_level, 2);
        assert_eq!(config.test.sandbox, "strict");
        assert_eq!(config.agent.model, "claude-4");
    }

    #[test]
    fn test_ci_topological_order() {
        let config = AlmConfig::parse(FULL_CONFIG).unwrap();
        let ordered = config.ci_stages_ordered().unwrap();
        let names: Vec<&str> = ordered.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["lint", "test", "bench", "build", "deploy"]);
    }

    #[test]
    fn test_ci_circular_deps() {
        let yaml = "version: \"0.1.0\"\nproject:\n  name: x\nci:\n  stages:\n    - name: a\n      run: x\n      needs: [b]\n    - name: b\n      run: x\n      needs: [a]\n";
        let config = AlmConfig::parse(yaml).unwrap();
        assert!(config.ci_stages_ordered().is_err());
    }

    #[test]
    fn test_modules_and_deps() {
        let config = AlmConfig::parse(FULL_CONFIG).unwrap();
        assert_eq!(config.modules[0].deps.len(), 2);
        assert_eq!(config.modules[0].deps[1].features, vec!["postgres", "pool"]);
    }

    #[test]
    fn test_deploy_targets() {
        let config = AlmConfig::parse(FULL_CONFIG).unwrap();
        assert_eq!(config.deploy["staging"].provider, "container");
        assert_eq!(config.deploy["staging"].rollback.as_deref(), Some("auto"));
        assert_eq!(config.deploy["production"].canary_percent, Some(10));
    }

    #[test]
    fn test_alerts() {
        let config = AlmConfig::parse(FULL_CONFIG).unwrap();
        let alert = &config.metrics.alerts[0];
        assert_eq!(alert.name, "high_error_rate");
        assert_eq!(alert.notify, vec!["pagerduty", "slack"]);
    }
}
