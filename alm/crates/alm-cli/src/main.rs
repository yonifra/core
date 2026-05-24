//! ALM CLI — entry point for the ALM language toolchain.
//!
//! Commands:
//!   alm run <file>     — parse + interpret an ALM source file
//!   alm check <file>   — parse only, report errors
//!   alm test <file>    — run @test annotations in file
//!   alm repl           — interactive REPL

use std::env;
use std::fs;
use std::process;

use alm_agent::{PromptBuilder, SelfHealLoop, ModuleSummary};
use alm_config::AlmConfig;
use alm_interp::{run, Interpreter};
use alm_lint::Linter;
use alm_llvm::Codegen;
use alm_parser::Parser;
use alm_wasm::{WasmCompiler, WasmSandbox, EffectChecker};
use alm_wasm::sandbox::SandboxConfig;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    match args[1].as_str() {
        "run" => cmd_run(&args),
        "build" => cmd_build(&args),
        "check" => cmd_check(&args),
        "emit-ir" => cmd_emit_ir(&args),
        "test" => cmd_test(&args),
        "lint" => cmd_lint(&args),
        "ci" => cmd_ci(),
        "generate" => cmd_generate(&args),
        "meta" => cmd_meta(&args),
        "heal" => cmd_heal(&args),
        "sandbox" => cmd_sandbox(&args),
        "effects" => cmd_effects(&args),
        "repl" => cmd_repl(),
        "version" | "--version" | "-v" => {
            println!("alm 0.1.0-alpha");
        }
        "help" | "--help" | "-h" => print_usage(),
        other => {
            // If it looks like a file path, run it
            if other.ends_with(".alm") {
                run_file(other);
            } else {
                eprintln!("E300 unknown command: {other}");
                print_usage();
                process::exit(1);
            }
        }
    }
}

fn print_usage() {
    eprintln!("ALM 0.1.0-alpha — Assembly for Language Models");
    eprintln!();
    eprintln!("Usage: alm <command> [args]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  run <file.alm>     Execute ALM source (interpreted)");
    eprintln!("  build <file.alm>   Compile to native binary");
    eprintln!("  check <file.alm>   Parse and type-check only");
    eprintln!("  emit-ir <file.alm> Show LLVM IR");
    eprintln!("  test <file.alm>    Run @test blocks");
    eprintln!("  lint <file.alm>    Static analysis");
    eprintln!("  ci                 Run CI pipeline from alm.yaml");
    eprintln!("  generate <intent>  Generate ALM agent prompt");
    eprintln!("  meta <file.alm>    Generate .alm.meta summary");
    eprintln!("  heal <file.alm>    Self-heal compilation errors");
    eprintln!("  sandbox <file.alm> Run in WASM sandbox");
    eprintln!("  effects <file.alm> Check effect violations");
    eprintln!("  repl               Interactive mode");
    eprintln!("  version            Show version");
}

fn cmd_run(args: &[String]) {
    if args.len() < 3 {
        eprintln!("E301 usage: alm run <file.alm>");
        process::exit(1);
    }
    run_file(&args[2]);
}

fn run_file(path: &str) {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("E302 cannot read {path}: {e}");
            process::exit(1);
        }
    };

    match run(&source) {
        Ok(val) => {
            let s = format!("{val}");
            if s != "()" {
                println!("{s}");
            }
        }
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    }
}

fn cmd_check(args: &[String]) {
    if args.len() < 3 {
        eprintln!("E301 usage: alm check <file.alm>");
        process::exit(1);
    }

    let source = match fs::read_to_string(&args[2]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("E302 cannot read {}: {e}", args[2]);
            process::exit(1);
        }
    };

    match Parser::parse(&source) {
        Ok(prog) => {
            println!("OK: {} statements parsed", prog.stmts.len());
        }
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    }
}

fn cmd_test(args: &[String]) {
    if args.len() < 3 {
        eprintln!("E301 usage: alm test <file.alm>");
        process::exit(1);
    }

    let source = match fs::read_to_string(&args[2]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("E302 cannot read {}: {e}", args[2]);
            process::exit(1);
        }
    };

    let mut interp = Interpreter::new();
    match interp.eval_source(&source) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    }

    let results = &interp.env.test_results;
    if results.is_empty() {
        println!("No @test blocks found");
        return;
    }

    let mut passed = 0;
    let mut failed = 0;
    let mut total_us = 0u64;
    for r in results {
        total_us += r.duration_us;
        if r.passed {
            println!("  PASS  {} ({} us)", r.name, r.duration_us);
            passed += 1;
        } else {
            print!("  FAIL  {} ({} us)", r.name, r.duration_us);
            if let Some(err) = &r.error {
                print!(" — {err}");
            }
            println!();
            failed += 1;
        }
    }

    println!();
    println!("{passed} passed, {failed} failed, {} total ({total_us} us)", passed + failed);

    if failed > 0 {
        process::exit(1);
    }
}

fn cmd_repl() {
    println!("ALM 0.1.0-alpha REPL (type Ctrl-D to exit)");
    let mut interp = Interpreter::new();
    let stdin = std::io::stdin();

    loop {
        eprint!("alm> ");
        let mut line = String::new();
        match stdin.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match interp.eval_source(trimmed) {
                    Ok(val) => {
                        let s = format!("{val}");
                        if s != "()" {
                            println!("{s}");
                        }
                    }
                    Err(e) => eprintln!("{e}"),
                }
            }
            Err(e) => {
                eprintln!("E303 read error: {e}");
                break;
            }
        }
    }
}

fn cmd_build(args: &[String]) {
    if args.len() < 3 {
        eprintln!("E301 usage: alm build <file.alm> [-o output]");
        process::exit(1);
    }

    let input = &args[2];
    let output = if args.len() >= 5 && args[3] == "-o" {
        args[4].clone()
    } else {
        // Strip .alm extension for output name
        input.strip_suffix(".alm").unwrap_or(input).to_string()
    };

    let source = match fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("E302 cannot read {input}: {e}");
            process::exit(1);
        }
    };

    match Codegen::compile_to_executable(&source, &output) {
        Ok(()) => {
            println!("compiled: {output}");
        }
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    }
}

fn cmd_emit_ir(args: &[String]) {
    if args.len() < 3 {
        eprintln!("E301 usage: alm emit-ir <file.alm>");
        process::exit(1);
    }

    let source = match fs::read_to_string(&args[2]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("E302 cannot read {}: {e}", args[2]);
            process::exit(1);
        }
    };

    match Codegen::compile_to_ir(&source) {
        Ok(ir) => println!("{ir}"),
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    }
}

fn cmd_lint(args: &[String]) {
    if args.len() < 3 {
        eprintln!("E301 usage: alm lint <file.alm> [--strict]");
        process::exit(1);
    }

    let source = match fs::read_to_string(&args[2]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("E302 cannot read {}: {e}", args[2]);
            process::exit(1);
        }
    };

    let strict = args.iter().any(|a| a == "--strict");
    let diags = Linter::lint(&source, strict);

    if diags.is_empty() {
        println!("OK: no issues");
        return;
    }

    let mut errors = 0;
    for d in &diags {
        println!("  {d}");
        if d.level == alm_lint::Level::Error {
            errors += 1;
        }
    }

    println!();
    println!("{} issue(s) found", diags.len());

    if errors > 0 {
        process::exit(1);
    }
}

fn cmd_ci() {
    let config = match AlmConfig::find_and_load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("E304 {e}");
            process::exit(1);
        }
    };

    if config.ci.stages.is_empty() {
        println!("No CI stages defined in alm.yaml");
        return;
    }

    let stages = match config.ci_stages_ordered() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("E305 {e}");
            process::exit(1);
        }
    };

    println!("CI pipeline: {} stages", stages.len());
    println!();

    for stage in &stages {
        print!("  [{:>8}] {}", stage.name, stage.run);

        let status = std::process::Command::new("sh")
            .args(["-c", &stage.run])
            .status();

        match status {
            Ok(s) if s.success() => {
                println!(" ... PASS");
            }
            Ok(s) => {
                println!(" ... FAIL (exit {})", s.code().unwrap_or(-1));
                process::exit(1);
            }
            Err(e) => {
                println!(" ... ERROR ({e})");
                process::exit(1);
            }
        }
    }

    println!();
    println!("CI pipeline complete: {} stages passed", stages.len());
}

fn cmd_generate(args: &[String]) {
    if args.len() < 3 {
        eprintln!("E301 usage: alm generate <intent> [--json]");
        process::exit(1);
    }

    let config = match AlmConfig::find_and_load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("E304 {e}");
            process::exit(1);
        }
    };

    let intent = args[2..].iter()
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");

    let prompt = PromptBuilder::build(&config, &intent);

    if args.iter().any(|a| a == "--json") {
        println!("{}", PromptBuilder::to_json(&prompt));
    } else {
        println!("{}", PromptBuilder::to_text(&prompt));
    }
}

fn cmd_meta(args: &[String]) {
    if args.len() < 3 {
        eprintln!("E301 usage: alm meta <file.alm> [--json]");
        process::exit(1);
    }

    let source = match fs::read_to_string(&args[2]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("E302 cannot read {}: {e}", args[2]);
            process::exit(1);
        }
    };

    match ModuleSummary::from_source(&args[2], &source) {
        Ok(summary) => {
            if args.iter().any(|a| a == "--json") {
                println!("{}", summary.to_json());
            } else {
                print!("{}", summary.to_meta_text());
            }
        }
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    }
}

fn cmd_heal(args: &[String]) {
    if args.len() < 3 {
        eprintln!("E301 usage: alm heal <file.alm>");
        process::exit(1);
    }

    let source = match fs::read_to_string(&args[2]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("E302 cannot read {}: {e}", args[2]);
            process::exit(1);
        }
    };

    let config = AlmConfig::find_and_load().unwrap_or_else(|_| {
        AlmConfig::parse("version: \"0.1.0\"\nproject:\n  name: adhoc\nagent:\n  self_heal: true\n").unwrap()
    });

    let heal = SelfHealLoop::from_config(&config);
    let result = heal.run(&source, &config, |_prompt| {
        // In alpha: no LLM API call. Print prompt, return None.
        // Real integration would call Claude API here.
        eprintln!("(agent: no LLM backend configured — manual repair needed)");
        None
    });

    print!("{}", SelfHealLoop::format_result(&result));

    if !result.success {
        process::exit(1);
    }
}

fn cmd_sandbox(args: &[String]) {
    if args.len() < 3 {
        eprintln!("E301 usage: alm sandbox <file.alm>");
        process::exit(1);
    }

    let source = match fs::read_to_string(&args[2]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("E302 cannot read {}: {e}", args[2]);
            process::exit(1);
        }
    };

    // Check effects first
    let report = EffectChecker::check(&source, "strict");
    if !report.violations.is_empty() {
        eprintln!("Effect violations (sandbox:strict):");
        for v in &report.violations {
            eprintln!("  {v}");
        }
        process::exit(1);
    }

    // Compile to WASM and run in sandbox
    let config = SandboxConfig::default();
    let result = WasmSandbox::compile_and_run(&source, &config);

    if result.success {
        println!("sandbox: OK (returned {})", result.return_value.unwrap_or(0));
        if !result.metrics.is_empty() {
            println!("metrics:");
            for (name, val) in &result.metrics {
                println!("  {name} = {val}");
            }
        }
    } else {
        eprintln!("sandbox: FAILED — {}", result.error.unwrap_or_default());
        process::exit(1);
    }
}

fn cmd_effects(args: &[String]) {
    if args.len() < 3 {
        eprintln!("E301 usage: alm effects <file.alm> [--mode strict|relaxed]");
        process::exit(1);
    }

    let source = match fs::read_to_string(&args[2]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("E302 cannot read {}: {e}", args[2]);
            process::exit(1);
        }
    };

    let mode = args.iter()
        .position(|a| a == "--mode")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("strict");

    let report = EffectChecker::check(&source, mode);

    if report.effects.is_empty() {
        println!("Pure: no effects detected");
    } else {
        println!("Effects: {}", report.effects.iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join(", "));
    }

    if report.violations.is_empty() {
        println!("No violations (mode: {mode})");
    } else {
        println!("{} violation(s):", report.violations.len());
        for v in &report.violations {
            println!("  {v}");
        }
        process::exit(1);
    }
}
