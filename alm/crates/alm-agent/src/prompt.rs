//! Prompt Builder — generates structured prompts for LLMs to produce ALM code.
//!
//! Given an alm.yaml config + natural language intent, produces a prompt
//! containing: ALM grammar reference, available builtins, project context,
//! and the specific task.

use alm_config::AlmConfig;
use serde::{Deserialize, Serialize};

/// Structured prompt for LLM code generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPrompt {
    pub system: String,
    pub context: ProjectContext,
    pub task: String,
    pub constraints: Vec<String>,
    pub examples: Vec<CodeExample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectContext {
    pub name: String,
    pub targets: Vec<String>,
    pub modules: Vec<String>,
    pub deps: Vec<String>,
    pub metrics_backend: String,
    pub test_strategy: String,
    pub sandbox_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeExample {
    pub description: String,
    pub code: String,
}

pub struct PromptBuilder;

impl PromptBuilder {
    /// ALM grammar reference — compact, token-efficient.
    pub const GRAMMAR_REF: &str = r#"ALM Syntax Reference (0.1-alpha):

STRUCTURE: No keywords. 24 sigils + identifiers + literals.
  {block}  (group)  [collection]  ; separator  , list  . access

SIGILS:
  @ annotation   # metric   ! effect   ? fallible   ~ async
  $ env-ref      ^ move     & borrow   &* mut-borrow  * deref/loop
  > return       | match-arm  => transform  := stack-bind
  .. range  ?: elvis  ++ increment  == eq  != ne  >= ge  <= le
  -> arrow  <- draw-from

CONSTRUCTS:
  x = expr;              heap binding
  x := expr;             stack binding
  name(args) { body }    function/lambda
  ~name(args) { body }   async function
  @ann(args) { body }    annotation (test, deploy, svc, bench)
  #name                  metric counter
  #name++                metric increment
  >expr                  return
  expr?                  try (propagate error)
  expr?: default         elvis (default on error)
  expr | P => e          match arm
  *(n) { body }          loop n times
  *() { body }           infinite loop
  *x <- col { body }     for-each
  $VAR                   env variable
  Name{f:v, ...}         struct literal
  !io { body }           effect block

BUILTINS: assert assertEq assertNe assertGt assertLt print len retry type str

CONVENTIONS:
  - camelCase identifiers (NOT snake_case)
  - identifiers ≤12 chars
  - no imports (deps in alm.yaml)
  - ; after statements
  - // single-line comments only"#;

    /// Build a complete agent prompt from config + intent.
    pub fn build(config: &AlmConfig, intent: &str) -> AgentPrompt {
        let context = Self::extract_context(config);

        let system = format!(
            "You are an ALM code generator. You write ALM (Assembly for Language Models) source code.\n\
             Output ONLY valid ALM code. No markdown, no explanation, no comments unless needed.\n\
             \n{}\n\
             \nProject: {}\nTargets: {}\nTest sandbox: {}\n",
            Self::GRAMMAR_REF,
            config.project.name,
            config.project.targets.join(", "),
            config.test.sandbox,
        );

        let constraints = vec![
            "Output valid ALM syntax only".into(),
            "Use camelCase identifiers, max 12 chars".into(),
            format!("Test strategy: {}", config.test.strategy),
            format!("Sandbox mode: {}", config.test.sandbox),
            "Include @test blocks for testable logic".into(),
            "Use #metric for observable operations".into(),
        ];

        AgentPrompt {
            system,
            context,
            task: intent.to_string(),
            constraints,
            examples: Self::default_examples(),
        }
    }

    /// Build a repair prompt after compilation failure.
    pub fn build_repair(
        config: &AlmConfig,
        original_code: &str,
        error: &str,
        attempt: u32,
        max_attempts: u32,
    ) -> AgentPrompt {
        let context = Self::extract_context(config);

        let system = format!(
            "You are an ALM code repair agent. Fix the compilation error.\n\
             Output ONLY the corrected ALM code. No explanation.\n\
             \n{}\n\
             \nAttempt {attempt}/{max_attempts}. Fix precisely — do not rewrite from scratch.\n",
            Self::GRAMMAR_REF,
        );

        let task = format!(
            "The following ALM code failed to compile:\n\
             ```\n{original_code}\n```\n\
             \nError:\n{error}\n\
             \nFix the error and output corrected ALM code.",
        );

        AgentPrompt {
            system,
            context,
            task,
            constraints: vec![
                "Fix ONLY the error — minimal changes".into(),
                "Preserve all working logic".into(),
                "Output valid ALM syntax".into(),
            ],
            examples: vec![],
        }
    }

    fn extract_context(config: &AlmConfig) -> ProjectContext {
        let modules: Vec<String> = config.modules.iter()
            .map(|m| format!("{}/{}", m.path, m.entry))
            .collect();

        let deps: Vec<String> = config.modules.iter()
            .flat_map(|m| m.deps.iter())
            .map(|d| {
                if d.features.is_empty() {
                    format!("{} v{}", d.name, d.ver)
                } else {
                    format!("{} v{} [{}]", d.name, d.ver, d.features.join(","))
                }
            })
            .collect();

        ProjectContext {
            name: config.project.name.clone(),
            targets: config.project.targets.clone(),
            modules,
            deps,
            metrics_backend: config.metrics.backend.clone(),
            test_strategy: config.test.strategy.clone(),
            sandbox_mode: config.test.sandbox.clone(),
        }
    }

    fn default_examples() -> Vec<CodeExample> {
        vec![
            CodeExample {
                description: "HTTP service with metrics".into(),
                code: "@svc(8080){~(r){#rq;p=proc(r)?;p|?=>retry(3,proc,r);>p}}".into(),
            },
            CodeExample {
                description: "Test block".into(),
                code: "@test(math){assertEq(2,2)}".into(),
            },
            CodeExample {
                description: "Binding + match".into(),
                code: "x=42;x|42=>\"found\"|_=>\"nope\"".into(),
            },
            CodeExample {
                description: "Struct + field access".into(),
                code: "w=Point{x:1,y:2};w.x".into(),
            },
            CodeExample {
                description: "Loop with metric".into(),
                code: "*(10){#iter}".into(),
            },
        ]
    }

    /// Render prompt as JSON (for API calls).
    pub fn to_json(prompt: &AgentPrompt) -> String {
        serde_json::to_string_pretty(prompt).unwrap_or_default()
    }

    /// Render prompt as flat text (for display/debugging).
    pub fn to_text(prompt: &AgentPrompt) -> String {
        let mut out = String::new();
        out.push_str("=== SYSTEM ===\n");
        out.push_str(&prompt.system);
        out.push_str("\n=== CONTEXT ===\n");
        out.push_str(&format!("Project: {}\n", prompt.context.name));
        out.push_str(&format!("Targets: {}\n", prompt.context.targets.join(", ")));
        out.push_str(&format!("Modules: {}\n", prompt.context.modules.join(", ")));
        out.push_str(&format!("Deps: {}\n", prompt.context.deps.join(", ")));
        out.push_str("\n=== TASK ===\n");
        out.push_str(&prompt.task);
        out.push_str("\n\n=== CONSTRAINTS ===\n");
        for c in &prompt.constraints {
            out.push_str(&format!("- {c}\n"));
        }
        if !prompt.examples.is_empty() {
            out.push_str("\n=== EXAMPLES ===\n");
            for ex in &prompt.examples {
                out.push_str(&format!("{}: {}\n", ex.description, ex.code));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AlmConfig {
        AlmConfig::parse(r#"
version: "0.1.0"
project:
  name: "test-app"
  targets: [linux-x86_64, darwin-arm64]
modules:
  - path: src/
    entry: main.alm
    deps:
      - name: http
        ver: "0.1"
test:
  strategy: unit
  sandbox: strict
metrics:
  backend: otlp
agent:
  model: claude-4
  self_heal: true
"#).unwrap()
    }

    #[test]
    fn test_build_prompt() {
        let config = test_config();
        let prompt = PromptBuilder::build(&config, "create an HTTP echo server");
        assert!(prompt.system.contains("ALM Syntax Reference"));
        assert!(prompt.system.contains("test-app"));
        assert_eq!(prompt.task, "create an HTTP echo server");
        assert!(!prompt.constraints.is_empty());
        assert!(!prompt.examples.is_empty());
    }

    #[test]
    fn test_prompt_contains_grammar() {
        let config = test_config();
        let prompt = PromptBuilder::build(&config, "anything");
        assert!(prompt.system.contains("@"));
        assert!(prompt.system.contains("#"));
        assert!(prompt.system.contains("camelCase"));
    }

    #[test]
    fn test_repair_prompt() {
        let config = test_config();
        let prompt = PromptBuilder::build_repair(
            &config,
            "x = ;",
            "E100 parse error at 1:5: expected expression, got ;",
            1, 3,
        );
        assert!(prompt.system.contains("repair"));
        assert!(prompt.task.contains("x = ;"));
        assert!(prompt.task.contains("E100"));
        assert!(prompt.system.contains("1/3"));
    }

    #[test]
    fn test_to_json() {
        let config = test_config();
        let prompt = PromptBuilder::build(&config, "hello");
        let json = PromptBuilder::to_json(&prompt);
        assert!(json.contains("\"task\""));
        assert!(json.contains("hello"));
    }

    #[test]
    fn test_to_text() {
        let config = test_config();
        let prompt = PromptBuilder::build(&config, "build a counter");
        let text = PromptBuilder::to_text(&prompt);
        assert!(text.contains("=== SYSTEM ==="));
        assert!(text.contains("=== TASK ==="));
        assert!(text.contains("build a counter"));
    }

    #[test]
    fn test_context_extraction() {
        let config = test_config();
        let prompt = PromptBuilder::build(&config, "x");
        assert_eq!(prompt.context.name, "test-app");
        assert_eq!(prompt.context.targets.len(), 2);
        assert_eq!(prompt.context.modules, vec!["src//main.alm"]);
        assert!(prompt.context.deps[0].contains("http"));
    }
}
