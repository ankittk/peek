// Color output helpers; respect NO_COLOR env and --no-color (set as NO_COLOR at startup).

use owo_colors::OwoColorize;

fn disabled() -> bool {
    std::env::var("NO_COLOR").is_ok()
}

pub fn red(s: &str) -> String {
    if disabled() {
        s.to_string()
    } else {
        s.red().to_string()
    }
}

pub fn bold(s: &str) -> String {
    if disabled() {
        s.to_string()
    } else {
        s.bold().to_string()
    }
}

pub fn yellow(s: &str) -> String {
    if disabled() {
        s.to_string()
    } else {
        s.yellow().to_string()
    }
}

pub fn green(s: &str) -> String {
    if disabled() {
        s.to_string()
    } else {
        s.green().to_string()
    }
}

pub fn cyan(s: &str) -> String {
    if disabled() {
        s.to_string()
    } else {
        s.cyan().to_string()
    }
}

pub fn dimmed(s: &str) -> String {
    if disabled() {
        s.to_string()
    } else {
        s.dimmed().to_string()
    }
}

pub fn red_bold(s: &str) -> String {
    if disabled() {
        s.to_string()
    } else {
        s.red().bold().to_string()
    }
}

pub fn yellow_bold(s: &str) -> String {
    if disabled() {
        s.to_string()
    } else {
        s.yellow().bold().to_string()
    }
}

pub fn cyan_bold(s: &str) -> String {
    if disabled() {
        s.to_string()
    } else {
        s.cyan().bold().to_string()
    }
}
