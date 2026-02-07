## Concept From
Nothing happens overnight. We don't randomly decide what components we need. First there's a scenario, then a requirement, and finally we build the component to solve it. In this document, we explain the principles of building a fuzzer step by step: why we need an observer, why we introduce a scheduler, objective, etc. Let's understand the design evolution of LibAFL from the perspective of problem → requirement → solution.

### 🎯 1. LibAFL's Design Evolution

#### Stage 0: The Original Idea
Problem: I want to test a function and see when it crashes.
The simplest solution:

```rust
fn naive_fuzzing() {
    let harness = |input: &[u8]| {
        target_function(input);  // Run the target function
    };
    
    // Infinitely generate random inputs and execute
    loop {
        let random_input = generate_random_bytes();
        harness(random_input);
    }
}
```
Problems arise:

- ❌ No way to know if we found something new; repeating the same code paths wastes time
- ❌ No way to remember inputs that cause crashes (our goals)



#### Stage 1: Introducing Coverage Tracking
New Requirement: I need to know if the test discovered a new code path!

Why?

- If an input executes a new code path → This is valuable! Worth saving and mutating from
- If an input executes a path we've seen → Boring, discard it

Solution:

```rust
// Global array to track if each code path has been executed
static mut SIGNALS: [u8; 16] = [0; 16];  // ← This is why we need a coverage map!

fn target_function_with_coverage(input: &[u8]) {
    signals_set(0);  // Record: entering function
    
    if input[0] == 'a' {
        signals_set(1);  // Record: executed this branch
        // ... more code
    }
}
```
Component Introduced: Observer (Watcher) ← Why do we need it?
- Someone needs to "observe" the changes in the SIGNALS array
- Used to compare the state "before" and "after" execution
- Determine if new bits are set to 1



#### Stage 2: Introducing Feedback (Feedback Mechanism)
New Problem: Observer told me there's new coverage, but I don't know how to use this information!

Why do we need Feedback?

- Observer only observes and records data
- Feedback is responsible for decision-making: is this input worth saving?
- Create a feedback loop: coverage increase → save input → mutate from it next time

Solution:

```rust
// Feedback core logic
let mut feedback = MaxMapFeedback::new(&observer);

// After executing a test:
if feedback.is_interesting(&observer) {  // ← Feedback makes the decision
    // This input discovered a new path! Save it
    corpus.add(input);
} else {
    // Boring input, discard it
}
```
What does this solve:
- ✅ Now we have a set of "interesting inputs" (Corpus)
- ✅ Fuzzer can mutate from interesting inputs



#### Stage 3: Introducing Scheduler (Scheduler)
New Problem: We have so many interesting inputs in the corpus, which one should I mutate next?

Why do we need a Scheduler?

- The number of inputs has increased, and order matters!
- Different inputs may have different "priority"
- Need a strategy to decide execution order

Different scheduling strategies:

```
QueueScheduler: FIFO, process in order
↓
WeightedScheduler: Weight based on certain metrics
↓
MinimizeScheduler: Prioritize smaller inputs (reduce redundancy)
```
Why is this critical?
- The same Fuzzer can have vastly different results based on selection strategy
- Some paths may require a specific sequence of inputs to reach



#### Stage 4: Introducing Mutator (Mutator)
New Problem: I'm only generating inputs randomly, that's very inefficient!

Why do we need a Mutator?

- Since we already have inputs in Corpus that can reach certain code paths, why start from scratch generating new ones?
- Idea: Make small modifications to existing "good inputs", which might discover new paths

Why is the Havoc mutation strategy effective?

```
Original input: "ab"

After mutation, we might get:
- BitFlip:      "ab" → "ac" (flip certain bit)
- ByteInsert:   "ab" → "acb" (insert byte)
- ByteDelete:   "ab" → "a" (delete byte)
- ByteRand:     "ab" → "xb" (replace with random byte)

➜ These mutations are "adjacent" explorations of the space
➜ More likely to find new paths than completely random
```



#### Stage 5: Introducing Objective (Objective Feedback)
New Problem: What I really want to find is input that causes a crash, but Feedback only cares about coverage!

Why do we need to separate Feedback and Objective?

Two different goals:

```
Feedback (Coverage Feedback):
  - Guide search direction
  - "Is this input interesting? Did we find a new path?"
  - Used to optimize the Fuzzer's search
  
Objective (Objective Feedback):
  - Detect success condition
  - "Did this input cause a crash?"
  - Used to discover vulnerabilities
```
Why can't we merge them?
```
If only Feedback exists, the Fuzzer will:
  ① Keep seeking new coverage
  ② But might never find a crash
  ③ Search space too large, no clear target

After separation:
  ① Feedback optimizes search efficiency
  ② Objective clearly indicates the goal
  ③ They complement each other
```



#### Stage 6: Introducing State (State Management)
New Problem: Too many components, state is getting too complex!
Why do we need State?

```
Data we need to manage:
├── Corpus (Corpus)
├── RNG state (Random Number Generator)
├── Feedback state (Coverage information)
├── Objective state (Crash information)
├── Execution statistics
├── Metadata
└── ...

What to do?
→ Create a central data structure: State
→ All components read/write data from State
→ Easy to extend, debug, serialize
```
Benefits of State:
- ✅ Unified communication between modules
- ✅ Easy to save/restore fuzzing progress
- ✅ Enables multi-process/multi-machine collaboration



#### Stage 7: Introducing Executor (Executor)
New Problem: How to execute the target function has become complex!

Why do we need an Executor?

Different execution methods have different requirements:

```
InProcessExecutor:
  ✓ Fast (same process)
  ✗ Target crash crashes the Fuzzer

ForkserverExecutor:
  ✓ Safe (target in subprocess)
  ✗ Slower (Fork overhead)

QemuExecutor:
  ✓ Supports binaries (no source code)
  ✗ Slowest (emulation overhead)

FridaExecutor:
  ✓ Dynamic instrumentation (no recompile)
  ✗ High complexity
```
Executor's responsibilities:
```rust
Executor::execute(input) {
    1. Call Harness function
    2. Collect data from all Observers
    3. Handle execution results (crash, timeout, etc.)
    4. Return unified result format
}
```
Why abstract?
- ✅ Can easily switch execution methods
- ✅ Core fuzzing logic remains unchanged



#### Stage 8: Introducing Stage (Stage)
New Problem: How to organize the interaction between Mutator and Executor?

Why do we need Stage?

```
A complete "mutate + execute" workflow:
    
    Stage.perform() {
        1. Get an input
        2. Apply Mutator
        3. Call Executor to execute
        4. Collect Observer data
        5. Call Feedback to evaluate
        6. Possibly save to Corpus
    }
```
Benefits of unified Stage interface:
- ✅ Can combine multiple Stages
- ✅ Can implement complex fuzzing strategies (e.g., multi-layer mutations)



#### Stage 9: Introducing Monitor and EventManager (Monitoring and Events)
New Problem: The Fuzzer is running but I don't know what's happening!

Why do we need Monitor?

- How many iterations have run?
- How big is the corpus?
- What's the execution speed?
- Have we found any vulnerabilities?

Why do we need EventManager?
```
Imagine distributed fuzzing across multiple machines:
  ├── Fuzzer on Machine A
  ├── Fuzzer on Machine B
  └── Fuzzer on Machine C

They need to communicate!
  - "I found a new interesting input"
  - "I found a crash"
  - Share corpus
  
EventManager is this communication coordinator
```



#### Stage 10: Introducing Fuzzer (Coordinator)
New Problem: So many components, who coordinates them?

Fuzzer's core responsibilities:

```rust
fn fuzz_loop() {
    loop {
        // 1. Scheduler selects input
        let input = scheduler.pick(&corpus);
        
        // 2. Stage mutates and executes
        stages.perform(&input, executor, state);
        
        // 3. Feedback evaluates
        if feedback.is_interesting() {
            corpus.add(input);
        }
        
        // 4. Objective checks
        if objective.found_crash() {
            save_crash();
        }
        
        // 5. Event management
        event_manager.handle_events();
    }
}
```
Why is this order critical?
- ✅ Reflects the complete feedback loop
- ✅ Shows how components work together



### 🎓  2. LibAFL Design Philosophy Summary

The evolution chain from requirements to components

```
Problem 1: Need to test function
  ↓ Introduce Harness

Problem 2: Don't know if we found new paths
  ↓ Introduce Observer + Coverage Map

Problem 3: Don't know how to use coverage information
  ↓ Introduce Feedback

Problem 4: Have many inputs, selection order matters
  ↓ Introduce Scheduler

Problem 5: Random generation too slow
  ↓ Introduce Mutator

Problem 6: Feedback only cares about coverage, not bugs
  ↓ Introduce Objective

Problem 7: Component communication is complex
  ↓ Introduce State

Problem 8: Different execution methods
  ↓ Introduce Executor

Problem 9: How to organize mutations and execution
  ↓ Introduce Stage

Problem 10: Can't see running state, multi-machine collaboration difficult
  ↓ Introduce Monitor + EventManager

Problem 11: So many components, who coordinates
  ↓ Introduce Fuzzer (Coordinator)
```

Core design principles

| Principle | Manifestation | Benefit |
| :------- | :-------------------------------------- | :------------------- |
| Modularity | Each component has single responsibility | Easy to understand, test, replace |
| Composability | Loose coupling between components | Can flexibly combine different strategies |
| Feedback Loop | Observer → Feedback → State → Scheduler | Forms optimization cycle |
| Strategy Pattern | Scheduler, Mutator, Executor all replaceable | Adapt to different scenarios |
| Coordination Pattern | Fuzzer as coordinator | Unified workflow |

### 💡 3. Why Is This Design So Elegant?

1. Build complexity layer by layer: Instead of cramming all concepts at once, start from the most basic problem and solve incrementally
2. Each component has a clear responsibility:
  - Observer = Data collection
  - Feedback = Decision making
  - Scheduler = Strategy execution
  - State = Information storage
  - Executor = Action execution
  - Mutator = Exploration generation

3. Easy to extend
  - Want to change search strategy? → Swap a Scheduler
  - Want to change execution method? → Swap an Executor
  - Want to change mutation method? → Swap a Mutator
  - Core Fuzzer logic remains unchanged

4. Visualization friendly
  - The entire flow is clear
  - Data flow direction is explicit
  - Easy to understand and debug


### 🎯 5. Key Points You Can Understand Now
✅ Why Observer must exist

Because we need to observe coverage changes

✅ Why Scheduler must exist

Because selection order affects efficiency and effectiveness

✅ Why Objective must be independent

Because "coverage" and "bugs" are two different goals

✅ Why we need State

Because there are too many components, we need a unified communication method

✅ Why this order

Each one solves a new problem introduced by the previous one
This is requirement-driven system design! 🚀
