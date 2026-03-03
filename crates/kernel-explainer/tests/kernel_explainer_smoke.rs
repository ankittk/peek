use kernel_explainer::{
    capabilities::format_caps,
    namespaces::namespace_description,
    oom::oom_description,
    scheduler::scheduler_description,
    states::state_description,
    well_known::binary_description,
};

#[test]
fn oom_description_covers_bands() {
    assert!(oom_description(-1).contains("unlikely"));
    assert!(oom_description(50).contains("Low"));
    assert!(oom_description(300).contains("Moderate"));
    assert!(oom_description(800).contains("High"));
}

#[test]
fn state_description_maps_known_states() {
    assert_eq!(state_description('R'), "Running");
    assert!(state_description('Z').contains("Zombie"));
}

#[test]
fn scheduler_description_has_fallback() {
    let s = scheduler_description("SCHED_OTHER");
    assert!(s.contains("Normal"));
    let unknown = scheduler_description("weird");
    assert!(unknown.contains("weird"));
}

#[test]
fn namespace_description_handles_known_and_unknown() {
    assert!(namespace_description("pid").contains("Process ID"));
    assert!(namespace_description("custom").contains("custom"));
}

#[test]
fn capabilities_formatting_handles_empty_and_bits() {
    let (perm, eff) = format_caps(0, 0);
    assert_eq!(perm, "none");
    assert_eq!(eff, "none");

    let (perm, _eff) = format_caps(1 << 0, 0);
    assert!(perm.contains("CHOWN"));
}

#[test]
fn binary_description_returns_some_for_known() {
    assert!(binary_description("nginx").is_some());
    assert!(binary_description("this-will-not-exist").is_none());
}

