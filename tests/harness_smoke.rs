//! Smoke test for the shared harness: sandbox isolation, fixed clock/RNG,
//! and publication-stage fault injection.

use std::rc::Rc;

use omd::testing::{
    Clock, FaultInjector, FixedClock, FixedRng, PublishStage, Rng, Sandbox, Timestamp,
};

#[test]
fn sandbox_isolated_and_removable() {
    let sb = Sandbox::new().unwrap();
    let root = sb.root().to_path_buf();
    sb.write_source_str("docs/a.md", "hello").unwrap();
    assert_eq!(sb.read_source("docs/a.md").unwrap(), b"hello");
    assert!(sb.meta_dir().parent().is_some());
    drop(sb);
    assert!(!root.exists());
}

#[test]
fn fixed_clock_and_rng_are_deterministic() {
    let c1 = FixedClock::new(Timestamp(1_000)).with_step(250);
    let c2 = FixedClock::new(Timestamp(1_000)).with_step(250);
    let r1 = FixedRng::new(7);
    let r2 = FixedRng::new(7);
    let (mut a, mut b) = ([0u8; 32], [0u8; 32]);
    r1.fill(&mut a);
    r2.fill(&mut b);
    assert_eq!(a, b);
    for _ in 0..4 {
        assert_eq!(c1.now(), c2.now());
    }
}

#[test]
fn fault_injector_hits_only_armed_stage_once() {
    let mut f = FaultInjector::new();
    f.arm_once(PublishStage::RenameState, 1);
    assert!(!f.hit(PublishStage::WriteRecords));
    assert!(!f.hit(PublishStage::WriteTempState));
    assert!(f.hit(PublishStage::RenameState));
    assert!(!f.hit(PublishStage::RenameState));
    assert!(!f.hit(PublishStage::SyncDir));
}

#[test]
fn omd_binary_exits_with_usage_error() {
    let sb = Rc::new(Sandbox::new().unwrap());
    let (code, _stderr) = omd::testing::OmdCmd::in_sandbox(sb).fail();
    assert_eq!(code, 2);
}
