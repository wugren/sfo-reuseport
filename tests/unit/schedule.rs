#[test]
fn scheduling_is_private_and_deterministic() {
    let core = include_str!("../../src/core/mod.rs");
    let lib = include_str!("../../src/lib.rs");
    let schedule = include_str!("../../src/core/schedule.rs");

    assert!(!core.contains("pub use schedule"));
    assert!(!lib.contains("DispatchPolicy"));
    assert!(!schedule.contains("DefaultHasher"));
    assert!(schedule.contains("linux_reuseport_select"));
}
