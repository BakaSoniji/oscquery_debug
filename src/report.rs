use std::fmt;

fn print_tagged(tag: &str, msg: &str) {
    let prefix = format!("[{tag}] ");
    let indent = " ".repeat(prefix.len());
    let mut first = true;
    for line in msg.lines() {
        let trimmed = line.trim_start();
        if first {
            println!("{prefix}{trimmed}");
            first = false;
        } else {
            println!("{indent}{trimmed}");
        }
    }
}

fn eprint_tagged(tag: &str, msg: &str) {
    let prefix = format!("[{tag}] ");
    let indent = " ".repeat(prefix.len());
    let mut first = true;
    for line in msg.lines() {
        let trimmed = line.trim_start();
        if first {
            eprintln!("{prefix}{trimmed}");
            first = false;
        } else {
            eprintln!("{indent}{trimmed}");
        }
    }
}

pub fn info(msg: impl fmt::Display) {
    print_tagged("INFO", &msg.to_string());
}

pub fn warn(msg: impl fmt::Display) {
    print_tagged("WARN", &msg.to_string());
}

pub fn pass(msg: impl fmt::Display) {
    print_tagged("PASS", &msg.to_string());
}

pub fn fail(msg: impl fmt::Display) {
    print_tagged("FAIL", &msg.to_string());
}


pub fn error(msg: impl fmt::Display) {
    eprint_tagged("ERR ", &msg.to_string());
}
