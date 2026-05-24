//! Self-Heal Loop — compile, detect errors, generate repair prompt, retry.
//!
//! The core agent lifecycle:
//! 1. Agent generates ALM code
//! 2. Compiler attempts to parse/check
//! 3. If error → build repair prompt with error context
//! 4. Agent fixes code
//! 5. Repeat up to N times

use alm_config::AlmConfig;
use alm_interp::Interpreter;
use alm_lint::Linter;
use alm_parser::Parser;

use crate::prompt::PromptBuilder;

/// Result of a single compilation attempt.
#[derive(Debug, Clone)]
pub struct CompileResult {
    pub success: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub test_passed: u32,
    pub test_failed: u32,
}

/// Result of the full self-heal loop.
#[derive(Debug, Clone)]
pub struct HealResult {
    pub final_code: String,
    pub success: bool,
    pub attempts: u32,
    pub max_attempts: u32,
    pub history: Vec<AttemptRecord>,
}

#[derive(Debug, Clone)]
pub struct AttemptRecord {
    pub attempt: u32,
    pub code: String,
    pub result: CompileResult,
    pub repair_prompt: Option<String>,
}

pub struct SelfHealLoop {
    pub max_attempts: u32,
}

impl SelfHealLoop {
    pub fn new(max_attempts: u32) -> Self {
        Self { max_attempts }
    }

    pub fn from_config(config: &AlmConfig) -> Self {
        let max = if config.agent.self_heal { 3 } else { 1 };
        Self::new(max)
    }

    /// Try to compile ALM source. Returns structured result.
    pub fn try_compile(source: &str) -> CompileResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Phase 1: Parse
        let prog = match Parser::parse(source) {
            Ok(p) => p,
            Err(e) => {
                return CompileResult {
                    success: false,
                    errors: vec![e.to_string()],
                    warnings: vec![],
                    test_passed: 0,
                    test_failed: 0,
                };
            }
        };

        // Phase 2: Lint
        let diags = Linter::lint(source, false);
        for d in &diags {
            match d.level {
                alm_lint::Level::Error => errors.push(d.to_string()),
                alm_lint::Level::Warning => warnings.push(d.to_string()),
            }
        }

        if !errors.is_empty() {
            return CompileResult {
                success: false,
                errors,
                warnings,
                test_passed: 0,
                test_failed: 0,
            };
        }

        // Phase 3: Interpret (run tests)
        let mut interp = Interpreter::new();
        match interp.eval_source(source) {
            Ok(_) => {}
            Err(e) => {
                errors.push(e.to_string());
            }
        }

        let test_passed = interp.env.test_results.iter().filter(|r| r.passed).count() as u32;
        let test_failed = interp.env.test_results.iter().filter(|r| !r.passed).count() as u32;

        for r in &interp.env.test_results {
            if !r.passed {
                errors.push(format!("test '{}' failed: {}", r.name, r.error.as_deref().unwrap_or("unknown")));
            }
        }

        CompileResult {
            success: errors.is_empty(),
            errors,
            warnings,
            test_passed,
            test_failed,
        }
    }

    /// Run the self-heal loop.
    /// `code_gen` is a callback that takes an optional repair prompt and returns new ALM code.
    /// In production this calls an LLM API; for testing it can be a simple function.
    pub fn run<F>(&self, initial_code: &str, config: &AlmConfig, mut code_gen: F) -> HealResult
    where
        F: FnMut(Option<&str>) -> Option<String>,
    {
        let mut current_code = initial_code.to_string();
        let mut history = Vec::new();

        for attempt in 1..=self.max_attempts {
            let result = Self::try_compile(&current_code);

            let repair_prompt = if !result.success && attempt < self.max_attempts {
                let error_text = result.errors.join("\n");
                let prompt = PromptBuilder::build_repair(
                    config,
                    &current_code,
                    &error_text,
                    attempt,
                    self.max_attempts,
                );
                Some(PromptBuilder::to_text(&prompt))
            } else {
                None
            };

            history.push(AttemptRecord {
                attempt,
                code: current_code.clone(),
                result: result.clone(),
                repair_prompt: repair_prompt.clone(),
            });

            if result.success {
                return HealResult {
                    final_code: current_code,
                    success: true,
                    attempts: attempt,
                    max_attempts: self.max_attempts,
                    history,
                };
            }

            // Try to get repaired code
            if let Some(ref prompt) = repair_prompt {
                if let Some(new_code) = code_gen(Some(prompt)) {
                    current_code = new_code;
                } else {
                    break; // code_gen returned None — give up
                }
            }
        }

        HealResult {
            final_code: current_code,
            success: false,
            attempts: self.max_attempts,
            max_attempts: self.max_attempts,
            history,
        }
    }

    /// Format heal result for display.
    pub fn format_result(result: &HealResult) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Agent: {} after {}/{} attempts\n",
            if result.success { "SUCCESS" } else { "FAILED" },
            result.attempts,
            result.max_attempts,
        ));

        for rec in &result.history {
            out.push_str(&format!("\n--- Attempt {} ---\n", rec.attempt));
            if rec.result.success {
                out.push_str(&format!(
                    "  PASS: {} tests passed\n",
                    rec.result.test_passed
                ));
            } else {
                for e in &rec.result.errors {
                    out.push_str(&format!("  ERROR: {e}\n"));
                }
            }
            for w in &rec.result.warnings {
                out.push_str(&format!("  WARN: {w}\n"));
            }
        }

        if result.success {
            out.push_str("\n--- Final Code ---\n");
            out.push_str(&result.final_code);
            out.push('\n');
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AlmConfig {
        AlmConfig::parse("version: \"0.1.0\"\nproject:\n  name: test\nagent:\n  self_heal: true\n").unwrap()
    }

    #[test]
    fn test_try_compile_valid() {
        let result = SelfHealLoop::try_compile("x = 42; x");
        assert!(result.success);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_try_compile_parse_error() {
        let result = SelfHealLoop::try_compile("x = ;");
        assert!(!result.success);
        assert!(!result.errors.is_empty());
        assert!(result.errors[0].contains("parse error"));
    }

    #[test]
    fn test_try_compile_with_tests() {
        let result = SelfHealLoop::try_compile("@test(ok){assertEq(1,1)};@test(fail){assertEq(1,2)}");
        assert!(!result.success);
        assert_eq!(result.test_passed, 1);
        assert_eq!(result.test_failed, 1);
    }

    #[test]
    fn test_self_heal_immediate_success() {
        let config = test_config();
        let heal = SelfHealLoop::new(3);
        let result = heal.run("x = 42", &config, |_| None);
        assert!(result.success);
        assert_eq!(result.attempts, 1);
    }

    #[test]
    fn test_self_heal_with_repair() {
        let config = test_config();
        let heal = SelfHealLoop::new(3);

        // Simulate: first code is broken, repair callback fixes it
        let result = heal.run("x = ;", &config, |prompt| {
            if prompt.is_some() {
                // "Agent" fixes the code
                Some("x = 42".into())
            } else {
                None
            }
        });

        assert!(result.success);
        assert_eq!(result.attempts, 2); // failed once, fixed on retry
        assert_eq!(result.final_code, "x = 42");
    }

    #[test]
    fn test_self_heal_exhausted() {
        let config = test_config();
        let heal = SelfHealLoop::new(2);

        // Agent keeps producing bad code
        let result = heal.run("x = ;", &config, |_| Some("y = ;".into()));

        assert!(!result.success);
        assert_eq!(result.attempts, 2);
    }

    #[test]
    fn test_format_result() {
        let config = test_config();
        let heal = SelfHealLoop::new(3);
        let result = heal.run("x = 42", &config, |_| None);
        let text = SelfHealLoop::format_result(&result);
        assert!(text.contains("SUCCESS"));
        assert!(text.contains("1/3"));
    }

    #[test]
    fn test_from_config() {
        let config = test_config();
        let heal = SelfHealLoop::from_config(&config);
        assert_eq!(heal.max_attempts, 3); // self_heal = true → 3 attempts
    }
}
