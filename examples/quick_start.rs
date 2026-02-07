use std::fs;
use std::path::Path;
use std::{path::PathBuf, ptr::write};

use libafl::monitors::SimpleMonitor;
use libafl::{
    corpus::{InMemoryCorpus, OnDiskCorpus},
    events::SimpleEventManager,
    executors::{ExitKind, InProcessExecutor},
    feedbacks::{CrashFeedback, MaxMapFeedback},
    fuzzer::{Fuzzer, StdFuzzer},
    generators::RandPrintablesGenerator,
    inputs::{BytesInput, HasTargetBytes},
    mutators::{havoc_mutations::havoc_mutations, scheduled::HavocScheduledMutator},
    observers::ConstMapObserver,
    schedulers::QueueScheduler,
    stages::mutational::StdMutationalStage,
    state::StdState,
};
use libafl_bolts::{
    current_nanos, nonnull_raw_mut, nonzero, rands::StdRand, tuples::tuple_list, AsSlice,
};

/// Coverage map with explicit assignments due to the lack of instrumentation
const SIGNALS_LEN: usize = 16;
static mut SIGNALS: [u8; SIGNALS_LEN] = [0; SIGNALS_LEN];
static mut SIGNALS_PTR: *mut u8 = &raw mut SIGNALS as _;

/// Assign a signal to the signals map
fn signals_set(idx: usize) {
    unsafe { write(SIGNALS_PTR.add(idx), 1) };
}

/// The closure that we want to fuzz
fn harness(input: &BytesInput) -> ExitKind {
    let target = input.target_bytes();
    let buf = target.as_slice();
    signals_set(0);
    if !buf.is_empty() && buf[0] == b'a' {
        signals_set(1);
        if buf.len() > 1 && buf[1] == b'b' {
            signals_set(2);
            if buf.len() > 2 && buf[2] == b'c' {
                panic!("Artificial bug triggered =)");
            }
        }
    }
    ExitKind::Ok
}

pub fn main() {
    delete_cache_files().expect("Failed to delete cache files");

    // Bind harness function to extend its lifetime
    let mut to_fuzz = harness;

    // Create an observation channel using the signals map
    let observer = unsafe { ConstMapObserver::from_mut_ptr("signals", nonnull_raw_mut!(SIGNALS)) };

    // Feedback to rate the interestingness of an input
    let mut feedback = MaxMapFeedback::new(&observer);

    // A feedback to choose if an input is a solution or not
    let mut objective = CrashFeedback::new();

    // create a State from scratch
    let mut state = StdState::new(
        // RNG
        StdRand::with_seed(current_nanos()),
        // Corpus that will be evolved, we keep it in memory for performance
        InMemoryCorpus::new(),
        // Corpus in which we store solutions (crashes in this example),
        // on disk so the user can get them after stopping the fuzzer
        OnDiskCorpus::new(PathBuf::from("./crashes")).unwrap(),
        // States of the feedbacks.
        // The feedbacks can report the data that should persist in the State.
        &mut feedback,
        // Same for objective feedbacks
        &mut objective,
    )
    .unwrap();

    // The Monitor trait define how the fuzzer stats are displayed to the user
    let mon = SimpleMonitor::new(|s| println!("{s}"));

    // The event manager handle the various events generated during the fuzzing loop
    // such as the notification of the addition of a new item to the corpus
    let mut mgr = SimpleEventManager::new(mon);

    // A queue policy to get testcasess from the corpus
    let scheduler = QueueScheduler::new();

    // A fuzzer with feedbacks and a corpus scheduler
    let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

    // Create the executor for an in-process function with just one observer
    let mut executor = InProcessExecutor::new(
        &mut to_fuzz,
        tuple_list!(observer),
        &mut fuzzer,
        &mut state,
        &mut mgr,
    )
    .expect("Failed to create the Executor");

    // Generator of printable bytearrays of max size 32
    let mut generator = RandPrintablesGenerator::new(nonzero!(32));

    // Generate 8 initial inputs
    state
        .generate_initial_inputs(&mut fuzzer, &mut executor, &mut generator, &mut mgr, 8)
        .expect("Failed to generate the initial corpus");

    // Setup a mutational stage with a basic bytes mutator
    let mutator = HavocScheduledMutator::new(havoc_mutations());
    let mut stages = tuple_list!(StdMutationalStage::new(mutator));

    fuzzer
        .fuzz_loop(&mut stages, &mut executor, &mut state, &mut mgr)
        .expect("Error in the fuzzing loop");
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
