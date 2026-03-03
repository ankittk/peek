// Scheduler policy → plain English (SCHED_OTHER, SCHED_FIFO, ...).
pub fn scheduler_description(policy: &str) -> String {
    match policy {
        // Generic/normal time-sharing (CFS)
        "normal" | "other" | "SCHED_OTHER" => {
            "Normal time-sharing scheduler (CFS) — balanced for general workloads".to_string()
        }
        // Background/batch work
        "batch" | "SCHED_BATCH" => {
            "Batch scheduler — optimized for non-interactive, CPU-intensive jobs".to_string()
        }
        // Very low priority
        "idle" | "SCHED_IDLE" => {
            "Idle scheduler — only runs when the CPU would otherwise be idle".to_string()
        }
        // Realtime policies
        "fifo" | "SCHED_FIFO" => {
            "Realtime FIFO — fixed-priority, first-in/first-out scheduling".to_string()
        }
        "rr" | "SCHED_RR" => {
            "Realtime round-robin — fixed-priority, time-sliced scheduling".to_string()
        }
        "deadline" | "SCHED_DEADLINE" => {
            "Realtime deadline — tasks scheduled to meet explicit deadlines".to_string()
        }
        other => format!("Scheduler policy: {}", other),
    }
}

#[cfg(test)]
mod tests {
    use super::scheduler_description;

    #[test]
    fn describes_known_policies() {
        let s = scheduler_description("SCHED_OTHER");
        assert!(s.contains("Normal time-sharing"));

        let s = scheduler_description("SCHED_BATCH");
        assert!(s.contains("Batch scheduler"));

        let s = scheduler_description("SCHED_FIFO");
        assert!(s.contains("Realtime FIFO"));
    }

    #[test]
    fn falls_back_for_unknown() {
        let s = scheduler_description("weird_policy");
        assert!(s.contains("weird_policy"));
    }
}
