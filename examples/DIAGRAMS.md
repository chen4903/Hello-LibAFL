# LibAFL Architecture Diagram Collection

## 1️⃣ Program Execution Flow Diagram (Timeline)

Shows the complete timeline from program startup to vulnerability discovery:

```mermaid
graph TD
    A["🎬 main() starts"] 
    --> B["📝 Define harness function<br/>Goal: panic when input is abc"]
    
    B --> C["👁️ Create Observer<br/>Monitor SIGNALS array"]
    C --> D["💭 Create Feedback<br/>MaxMapFeedback: coverage<br/>CrashFeedback: crashes"]
    D --> E["🎒 Create State<br/>Store: corpus, RNG, feedback state"]
    
    E --> F["📊 Create Monitor & EventMgr<br/>Show statistics, handle events"]
    F --> G["📋 Create Scheduler<br/>QueueScheduler: FIFO selection"]
    G --> H["🔧 Create Executor<br/>InProcessExecutor: in-process execution"]
    
    H --> I["🎲 Generate initial inputs<br/>RandPrintablesGenerator<br/>8 random byte sequences"]
    
    I --> J["🧬 Create Mutator<br/>HavocScheduledMutator<br/>Contains 16 mutation strategies"]
    J --> K["⚙️ Create Stage<br/>StdMutationalStage<br/>Responsible for mutation and execution"]
    
    K --> L["🔄 Start fuzz_loop()"]
    
    L --> M["Loop iteration begins"]
    
    M --> M1["Step 1️⃣<br/>Scheduler selects input<br/>FIFO selection from corpus"]
    M1 --> M2["Step 2️⃣<br/>Mutator mutates input<br/>Randomly modify byte sequence"]
    M2 --> M3["Step 3️⃣<br/>Executor executes<br/>Run harness function"]
    
    M3 --> M4["Step 4️⃣<br/>Observer monitors<br/>Collect SIGNALS array changes"]
    M4 --> M5["Step 5️⃣<br/>Feedback evaluates<br/>Determine if new path found"]
    
    M5 --> M6{"New path<br/>discovered?"}
    M6 -->|No| M8["Discard input<br/>Continue next round"]
    M6 -->|Yes| M7["💾 Save to corpus<br/>corpus count++"]
    
    M7 --> M9{"Triggered<br/>panic?"}
    M8 --> M9
    
    M9 -->|No| M10["Continue loop"]
    M9 -->|Yes| M11["🎯 Vulnerability found!<br/>objectives count++"]
    
    M11 --> M12["💾 Save to crashes/"]
    M12 --> M10
    
    M10 -->|Continue| M
    
    M10 -->|Manual stop| N["📈 Output final statistics<br/>Total executions, corpus size, etc"]
    N --> O["✅ Program ends"]
    
    style M fill:#fff3e0
    style M1 fill:#e3f2fd
    style M2 fill:#e3f2fd
    style M3 fill:#e3f2fd
    style M4 fill:#e3f2fd
    style M5 fill:#e3f2fd
    style M11 fill:#c8e6c9
    style M12 fill:#c8e6c9
```

---

## 2️⃣ Data Flow Diagram

Shows how data flows between components:

```mermaid
graph LR
    Input["🎲 Random input<br/>Byte sequence"]
    
    Input -->|Generate| Gen["🎲 Generator"]
    Gen -->|8 initial| Corpus["📚 Corpus<br/>InMemoryCorpus"]
    
    Corpus -->|FIFO select| Scheduler["📋 Scheduler"]
    Scheduler -->|Return one input| Mutator["🧬 Mutator"]
    
    Mutator -->|Mutate| MutatedInput["🔄 Mutated input<br/>New byte sequence"]
    MutatedInput -->|Send to| Executor["🔧 Executor"]
    
    Executor -->|Run| Harness["🎯 Harness"]
    Harness -->|Update| SIGNALS["📊 SIGNALS<br/>Coverage array"]
    
    SIGNALS -->|Monitor| Observer["👁️ Observer"]
    Observer -->|Feedback data| Feedback["💭 Feedback"]
    
    Harness -->|Return| ExitKind["📤 ExitKind<br/>Ok / Crash"]
    ExitKind -->|Evaluate| Objective["🎯 Objective"]
    
    Feedback -->|Determine| Decision1{"New path?"}
    Decision1 -->|Yes| SaveCorpus["💾 Save<br/>Corpus"]
    Decision1 -->|No| Discard["🗑️ Discard"]
    
    Objective -->|Determine| Decision2{"Crash?"}
    Decision2 -->|Yes| SaveCrash["💾 Save<br/>crashes/"]
    Decision2 -->|No| Continue["Continue"]
    
    SaveCorpus -->|Update| Corpus
    SaveCrash -->|Output| CrashDir["💥 crashes/ directory"]
    Discard -->|Continue| Scheduler
    Continue -->|Continue| Scheduler
    
    SaveCorpus -->|Statistics| Monitor["📊 Monitor"]
    SaveCrash -->|Statistics| Monitor
    Monitor -->|Display| Output["📈 Output statistics"]
```

---

## 3️⃣ Harness Function Execution Tree

Shows the target function's execution paths and coverage mapping:

```mermaid
graph TD
    Entry["🟢 Harness entry<br/>signals_set(0)"]
    
    Entry --> C1{"input[0]<br/>== 'a'?"}
    
    C1 -->|No| Exit1["Return normally<br/>ExitKind::Ok"]
    C1 -->|Yes| S1["signals_set(1)"]
    
    S1 --> C2{"input1<br/>== 'b'?"}
    
    C2 -->|No| Exit2["Return normally<br/>ExitKind::Ok"]
    C2 -->|Yes| S2["signals_set(2)"]
    
    S2 --> C3{"input2<br/>== 'c'?"}
    
    C3 -->|No| Exit3["Return normally<br/>ExitKind::Ok"]
    C3 -->|Yes| Panic["💥 panic!"]
    
    Panic -->|Trigger| Crash["🔴 CRASH<br/>Program abnormal exit"]
    Crash -->|By Objective| SaveCrash["💾 Save to crashes/"]
    
    Exit1 -->|No new signals| Discard1["Discard"]
    Exit2 -->|New signals| SaveCorpus2["Save to corpus"]
    Exit3 -->|New signals| SaveCorpus3["Save to corpus"]
    
    SaveCorpus2 -->|Next gen input| NextGen["🔄 Mutate from Corpus<br/>Mutate 'ab' → 'abc'"]
    SaveCorpus3 -->|Next gen input| NextGen
    
    NextGen -->|Continue Mutate| Entry
    
    style Entry fill:#c8e6c9
    style Exit1 fill:#ffcdd2
    style Exit2 fill:#ffcdd2
    style Exit3 fill:#ffcdd2
    style Crash fill:#d32f2f,color:#fff
    style SaveCrash fill:#c8e6c9
    style SaveCorpus2 fill:#bbdefb
    style SaveCorpus3 fill:#bbdefb
```

---

## 4️⃣ Feedback Mechanism Explained

Shows how coverage feedback guides the search:

```mermaid
graph TD
    Input["Input: some byte sequence<br/>e.g. 'xyz'"]
    
    Input -->|Execute| Execute["Execute Harness"]
    Execute -->|Collect| Before["Before execution<br/>SIGNALS state"]
    Execute -->|After execution| After["After execution<br/>SIGNALS state"]
    
    Before -->|Compare| Feedback["MaxMapFeedback<br/>Any new bits set to 1?"]
    After -->|Compare| Feedback
    
    Feedback -->|Compare| Check{"Any new bits<br/>in SIGNALS?"}
    
    Check -->|Yes| Interesting["✅ Interesting!<br/>Found new path"]
    Check -->|No| Boring["❌ Boring<br/>Repeated path"]
    
    Interesting -->|Save| CorpusList["📚 Corpus<br/>Interesting input list<br/>e.g. 'a', 'ab', 'abc'"]
    Boring -->|Discard| Discard["🗑️ Discard<br/>Don't mutate again"]
    
    CorpusList -->|Next| Mutate["Mutate all interesting inputs<br/>Generate more test cases"]
    Mutate -->|Loop| Feedback
    
    style Interesting fill:#c8e6c9
    style Boring fill:#ffcdd2
    style CorpusList fill:#bbdefb
```

---

## 5️⃣ Complete Mutation Process Diagram

Shows one complete mutation execution cycle:

```mermaid
graph TD
    A["📚 Corpus library<br/>Interesting input set<br/>'a', 'ab', 'abc'"]
    
    A -->|Select| B["📋 Scheduler<br/>QueueScheduler<br/>Return: 'ab'"]
    
    B -->|Get| C["Input: ab<br/>bytes: 0x61, 0x62"]
    
    C -->|Apply| D["🧬 Mutator<br/>Havoc mutation"]
    
    D -->|Havoc includes| D1["BitFlip<br/>Byte bit flip"]
    D -->|Havoc includes| D2["ByteFlip<br/>Whole byte flip"]
    D -->|Havoc includes| D3["ByteAdd<br/>Byte value increment"]
    D -->|Havoc includes| D4["ByteDec<br/>Byte value decrement"]
    D -->|Havoc includes| D5["ByteRand<br/>Random byte"]
    D -->|Havoc includes| D6["BytesDelete<br/>Delete bytes"]
    D -->|Havoc includes| D7["BytesInsert<br/>Insert bytes"]
    
    D1 -->|Example| D1_ex["BitFlip: 61,62<br/>After: 60,62"]
    D2 -->|Example| D2_ex["ByteAdd: 61,62<br/>After: 62,62"]
    D7 -->|Example| D7_ex["BytesInsert: 61,62<br/>After: 61,63,62"]
    
    D1_ex -->|Result| E1["Mutated Input<br/>0x60, 0x62"]
    D2_ex -->|Result| E2["Mutated Input<br/>0x62, 0x62"]
    D7_ex -->|Result| E3["Mutated Input<br/>0x61, 0x63, 0x62"]
    
    E1 -->|Execute| F["⚙️ Stage"]
    E2 -->|Execute| F
    E3 -->|Execute| F
    
    F -->|Run| G["🔧 Executor<br/>InProcessExecutor"]
    
    G -->|Call| H["🎯 Harness function"]
    
    H -->|Result 1| H1["Input: 0x60 0x62<br/>Not ab<br/>Boring"]
    H -->|Result 2| H2["Input: 0x62 0x62<br/>Not ab<br/>Boring"]
    H -->|Result 3| H3["Input: 0x61 0x63 0x62<br/>Not ab<br/>Boring"]
    
    H1 -->|Evaluate| I1["❌ Feedback: Boring"]
    H2 -->|Evaluate| I2["❌ Feedback: Boring"]
    H3 -->|Evaluate| I3["❌ Feedback: Boring"]
    
    I1 -->|Continue| J["🔄 Continue loop<br/>Scheduler select next"]
    I2 -->|Continue| J
    I3 -->|Continue| J
    
    J -->|Finally| K["Infinite loop<br/>Until 'abc' found"]
    
    style A fill:#bbdefb
    style B fill:#fff9c4
    style D fill:#f3e5f5
    style H fill:#e1f5fe
    style I1 fill:#ffcdd2
    style I2 fill:#ffcdd2
    style I3 fill:#ffcdd2
```
