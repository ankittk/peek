// Signal number → name and description for kill panel.
pub fn signal_description(num: i32) -> String {
    match num {
        15 => "SIGTERM (15): ask the process to exit cleanly; can be caught or handled.".to_string(),
        9 => "SIGKILL (9): forcefully kill the process; cannot be caught, blocked, or ignored."
            .to_string(),
        2 => "SIGINT (2): interrupt from keyboard (Ctrl-C); default is to terminate.".to_string(),
        3 => {
            "SIGQUIT (3): quit from keyboard; usually terminates and produces a core dump."
                .to_string()
        }
        1 => "SIGHUP (1): hangup; often used by daemons to reload configuration.".to_string(),
        19 => "SIGSTOP (19): stop (pause) the process; cannot be caught or ignored.".to_string(),
        18 => "SIGCONT (18): resume a process previously stopped with SIGSTOP/SIGTSTP.".to_string(),
        10 => {
            "SIGUSR1 (10): user-defined signal 1; semantics are application-specific.".to_string()
        }
        12 => {
            "SIGUSR2 (12): user-defined signal 2; semantics are application-specific.".to_string()
        }
        other => format!("Signal {}: see `kill -l` for details.", other),
    }
}

#[cfg(test)]
mod tests {
    use super::signal_description;

    #[test]
    fn describes_common_signals() {
        let term = signal_description(15);
        assert!(term.contains("SIGTERM"));

        let kill = signal_description(9);
        assert!(kill.contains("SIGKILL"));
    }

    #[test]
    fn describes_unknown_signal_generically() {
        let s = signal_description(99);
        assert!(s.contains("Signal 99"));
    }
}
