use libafl::{
    executors::ExitKind,
    inputs::{BytesInput, HasTargetBytes},
};
use libafl_bolts::AsSlice;
use revm::{
    context::TxEnv,
    database::State,
    primitives::{Address, Bytes, TxKind, U256},
    state::Bytecode,
    Context, ExecuteEvm, MainBuilder, MainContext,
};

use std::ptr::write;

// Coverage map with explicit assignments due to the lack of instrumentation
const SIGNALS_LEN: usize = 256;
pub static mut SIGNALS: [u8; SIGNALS_LEN] = [0; SIGNALS_LEN];
static mut SIGNALS_PTR: *mut u8 = &raw mut SIGNALS as _;

pub fn signals_set(idx: usize) {
    unsafe { write(SIGNALS_PTR.add(idx), 1) };
}

// ToFuzz.sol runtime bytecode (deployed contract code, without constructor)
const CONTRACT_BYTECODE: &str = "60808060405260043610156011575f80fd5b5f3560e01c908163890eba6814608e575063ebd4b2f914602f575f80fd5b34608a576020366003190112608a576064600435036055575f805460ff19166001179055005b60405162461bcd60e51b815260206004820152600d60248201526c078204d5553542062652031303609c1b6044820152606490fd5b5f80fd5b34608a575f366003190112608a5760209060ff5f541615158152f3fea264697066735822122049083d31998b256e45c3c0b46511efd039b44ab5ec0d8bb7f2514ba8b0330e6b64736f6c634300081e0033";
const CONTRACT_ADDRESS: Address = Address::new([0x13; 20]);
const CALLER_ADDRESS: Address = Address::new([0x37; 20]);

pub fn harness(input: &BytesInput) -> ExitKind {
    let target = input.target_bytes();
    let calldata = target.as_slice();

    // Skip too short inputs
    if calldata.len() < 4 {
        return ExitKind::Ok;
    }

    // Function selector (first 4 bytes)
    let selector = &calldata[0..4];
    signals_set(selector[0] as usize % SIGNALS_LEN);

    // Set contract code
    let bytecode_bytes = hex::decode(CONTRACT_BYTECODE).unwrap();
    let bytecode = Bytecode::new_raw(Bytes::from(bytecode_bytes));

    // Create revm instance
    let mut state_for_building = State::builder().with_bal_builder().build();
    state_for_building.insert_account(
        CONTRACT_ADDRESS,
        revm::state::AccountInfo {
            code_hash: bytecode.hash_slow(),
            code: Some(bytecode),
            ..Default::default()
        },
    );
    let ctx = Context::mainnet().with_db(&mut state_for_building);
    let mut evm = ctx.build_mainnet();

    let tx: TxEnv = TxEnv::builder()
        .caller(CALLER_ADDRESS)
        .kind(TxKind::Call(CONTRACT_ADDRESS))
        .data(Bytes::from(calldata.to_vec()))
        .gas_limit(1_000_000)
        .build()
        .unwrap();

    // Execute transaction (use transact instead of transact_commit to get state changes)
    match evm.transact(tx) {
        Ok(result_and_state) => {
            // Observe execution result
            signals_set(1);

            let result = &result_and_state.result;
            let state = &result_and_state.state;

            // Debug: print calldata for first few executions
            static mut EXEC_COUNT: u32 = 0;
            let should_debug = unsafe {
                EXEC_COUNT += 1;
                EXEC_COUNT <= 5
                    || (calldata.len() == 36 && calldata[0..4] == [0xeb, 0xd4, 0xb2, 0xf9])
            };

            if should_debug {
                eprintln!("\n=== DEBUG Execution ===");
                eprintln!("Calldata: {}", hex::encode(calldata));
                eprintln!("Success: {}", result.is_success());
                eprintln!("Gas used: {}", result.gas_used());
                eprintln!("State changes: {}", state.len());
            }

            // Check storage slot 0 (flag variable) for success condition
            if let Some(account_state) = state.get(&CONTRACT_ADDRESS) {
                if should_debug {
                    eprintln!("Account state found!");
                    eprintln!("Storage changes: {}", account_state.storage.len());
                }

                // Check storage slot 0
                if let Some(storage_slot) = account_state.storage.get(&U256::from(0)) {
                    let flag_value = storage_slot.present_value;

                    if should_debug {
                        eprintln!("Storage slot 0 value: {}", flag_value);
                    }

                    // Use storage value for coverage
                    let hash = flag_value.to::<u64>() as usize;
                    signals_set(hash % SIGNALS_LEN);

                    // Check if flag is true (slot-0 == 1)
                    if flag_value == U256::from(1) {
                        signals_set(3);
                        println!("\n🎉🎉🎉 FUZZING SUCCESS! 🎉🎉🎉");
                        println!("🎯 Storage slot 0 value: {}", flag_value);
                        println!("📝 Calldata (hex): {}", hex::encode(calldata));
                        panic!(
                            "✅ Flag is set to true! Winning input: {}",
                            hex::encode(calldata)
                        );
                    }
                } else if should_debug {
                    eprintln!("Storage slot 0 NOT in changes");
                }
            } else if should_debug {
                eprintln!("Account NOT in state changes");
            }

            // If revert, record it
            if !result.is_success() {
                signals_set(2);
            }
        }
        Err(e) => {
            eprintln!("Transaction error: {:?}", e);
            signals_set(4);
        }
    }

    ExitKind::Ok
}
