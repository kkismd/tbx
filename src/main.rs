use std::io::{self, Write};
use tbx::error::TbxError;
use tbx::interpreter::{Interpreter, InterpreterError};

fn print_error(err: &InterpreterError) {
    eprintln!("Error: {err}");
    eprintln!("  {}", err.source_line);
}

fn run_file(path: &str) -> std::process::ExitCode {
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: cannot read '{}': {}", path, e);
            return std::process::ExitCode::FAILURE;
        }
    };

    let mut interp = Interpreter::new();

    // Resolve the base directory from the input file's parent directory.
    // This makes relative USE paths inside the program file independent of
    // the process CWD.
    // Canonicalize the file path itself first to avoid the empty-parent issue
    // when only a bare filename is given (e.g. "foo.tbx" -> parent is "").
    if let Ok(abs_path) = std::fs::canonicalize(path) {
        if let Some(parent) = abs_path.parent() {
            // canonicalize always returns an absolute path, so parent() is
            // also absolute; set_base_dir will not return Err here in practice.
            if let Err(e) = interp.set_base_dir(parent.to_path_buf()) {
                eprintln!("Error: {e}");
                return std::process::ExitCode::FAILURE;
            }
        }
    }

    match interp.compile_program(&src) {
        Ok(()) => {
            let out = interp.take_output();
            print!("{out}");
            let _ = io::stdout().flush();
            std::process::ExitCode::SUCCESS
        }
        Err(err) => {
            // Flush any output that was produced before the error.
            let out = interp.take_output();
            print!("{out}");
            let _ = io::stdout().flush();
            print_error(&err);
            std::process::ExitCode::FAILURE
        }
    }
}

fn run_stdin() -> std::process::ExitCode {
    let mut interp = Interpreter::new();
    // Read from the VM's own input_reader instead of acquiring a separate
    // StdinLock.  This avoids the deadlock that occurs when ACCEPT tries to
    // re-lock stdin while the outer loop already holds the lock.
    let mut line_num: usize = 0;
    loop {
        match interp.read_input_line() {
            Ok(None) => break,
            Ok(Some(line)) => {
                line_num += 1;
                match interp.exec_line(&line, line_num) {
                    Ok(()) => {
                        let out = interp.take_output();
                        print!("{out}");
                        let _ = io::stdout().flush();
                    }
                    Err(err) if matches!(err.kind, TbxError::Halted) => {
                        let out = interp.take_output();
                        print!("{out}");
                        let _ = io::stdout().flush();
                        return std::process::ExitCode::SUCCESS;
                    }
                    Err(err) => {
                        print_error(&err);
                        return std::process::ExitCode::FAILURE;
                    }
                }
            }
            Err(e) => {
                eprintln!("Error: reading stdin: {}", e);
                return std::process::ExitCode::FAILURE;
            }
        }
    }

    std::process::ExitCode::SUCCESS
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();

    match args.as_slice() {
        [_] => run_stdin(),
        [_, path] => run_file(path),
        _ => {
            eprintln!("Usage: tbx [source_file]");
            std::process::ExitCode::FAILURE
        }
    }
}
