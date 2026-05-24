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

use alm_interp::{run, Interpreter};
use alm_parser::Parser;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    match args[1].as_str() {
        "run" => cmd_run(&args),
        "check" => cmd_check(&args),
        "test" => cmd_test(&args),
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
    eprintln!("  run <file.alm>     Execute ALM source");
    eprintln!("  check <file.alm>   Parse and type-check only");
    eprintln!("  test <file.alm>    Run @test blocks");
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
    for (name, ok) in results {
        if *ok {
            println!("  PASS  {name}");
            passed += 1;
        } else {
            println!("  FAIL  {name}");
            failed += 1;
        }
    }

    println!();
    println!("{passed} passed, {failed} failed, {} total", passed + failed);

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
