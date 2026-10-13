use libafl::{
    corpus::{InMemoryCorpus, OnDiskCorpus},
    events::SimpleEventManager,
    executors::inprocess::InProcessExecutor,
    feedbacks::{CrashFeedback, MaxMapFeedback},
    fuzzer::{Fuzzer, StdFuzzer},
    inputs::BytesInput,
    monitors::SimpleMonitor,
    mutators::{havoc_mutations::havoc_mutations, scheduled::HavocScheduledMutator},
    observers::ConstMapObserver,
    schedulers::QueueScheduler,
    stages::mutational::StdMutationalStage,
    state::StdState,
    Evaluator,
};
use libafl_bolts::{nonnull_raw_mut, rands::StdRand, tuples::tuple_list};
use std::{
    fs,
    path::{Path, PathBuf},
};
use Hello_LibAFL::harness::{harness, SIGNALS};

pub fn main() {
    delete_cache_files().expect("Failed to delete cache files");

    // Harness: execute contract using revm
    let mut to_fuzz = harness;

    // Create observer
    let observer = unsafe { ConstMapObserver::from_mut_ptr("signals", nonnull_raw_mut!(SIGNALS)) };

    // Create feedback
    let mut feedback = MaxMapFeedback::new(&observer);
    let mut objective = CrashFeedback::new();

    // Create state
    let mut state = StdState::new(
        StdRand::new(),
        InMemoryCorpus::new(),
        OnDiskCorpus::new(PathBuf::from("./crashes")).unwrap(),
        &mut feedback,
        &mut objective,
    )
    .unwrap();

    // Create monitor and event manager
    let mon = SimpleMonitor::new(|s| println!("{s}"));
    let mut mgr = SimpleEventManager::new(mon);

    // Create scheduler and fuzzer
    let scheduler = QueueScheduler::new();
    let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

    // Create executor
    let mut executor = InProcessExecutor::new(
        &mut to_fuzz,
        tuple_list!(observer),
        &mut fuzzer,
        &mut state,
        &mut mgr,
    )
    .expect("Failed to create executor");

    // Add initial seed inputs
    // Try(uint256) function selector = 0xebd4b2f9
    // Seed 1: Try(42) - wrong value, should revert
    let seed1 = BytesInput::new(
        hex::decode("ebd4b2f9000000000000000000000000000000000000000000000000000000000000002a")
            .unwrap(),
    );
    fuzzer
        .evaluate_input(&mut state, &mut executor, &mut mgr, &seed1)
        .unwrap();

    // Seed 2: Try(1) - wrong value, should revert
    let seed2 = BytesInput::new(
        hex::decode("ebd4b2f90000000000000000000000000000000000000000000000000000000000000001")
            .unwrap(),
    );
    fuzzer
        .evaluate_input(&mut state, &mut executor, &mut mgr, &seed2)
        .unwrap();

    // Seed 3: Try(99) - close to target, should revert
    let seed3 = BytesInput::new(
        hex::decode("ebd4b2f90000000000000000000000000000000000000000000000000000000000000063")
            .unwrap(),
    );
    fuzzer
        .evaluate_input(&mut state, &mut executor, &mut mgr, &seed3)
        .unwrap();

    // // TEST ONLY: Try(100) - the correct answer! (Remove this in real fuzzing)
    // println!("🧪 Testing with correct input Try(100) first...");
    // let test_correct = BytesInput::new(
    //     hex::decode("ebd4b2f90000000000000000000000000000000000000000000000000000000000000064")
    //         .unwrap(),
    // );
    // fuzzer
    //     .evaluate_input(&mut state, &mut executor, &mut mgr, &test_correct)
    //     .unwrap();

    // Create mutator and stage
    let mutator = HavocScheduledMutator::new(havoc_mutations());
    let mut stages = tuple_list!(StdMutationalStage::new(mutator));

    // Start fuzzing
    println!("Starting Solidity contract fuzzing...");
    fuzzer
        .fuzz_loop(&mut stages, &mut executor, &mut state, &mut mgr)
        .expect("Error in fuzzing loop");
}

pub fn delete_cache_files() -> std::io::Result<()> {
    let dirs = vec!["crashes", "corpus", "solutions", "minimized"];

    for dir_name in dirs {
        if Path::new(dir_name).exists() {
            fs::remove_dir_all(dir_name)?;
            println!("Deleted directory: {}/", dir_name);
        }
    }

    Ok(())
}
