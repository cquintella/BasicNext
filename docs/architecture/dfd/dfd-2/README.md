# DFD-2 — one file per DFD-1 process (TO-BE)

Parent diagram: [../dfd-1-to-be.md](../dfd-1-to-be.md)  
Data dictionary: [../data-dictionary.md](../data-dictionary.md)

| File | Opens |
| --- | --- |
| [1.0 Control.md](1.0 Control.md) | 1.0 Control (`bnc`) — flows `C01`–`C28` |
| [2.0 Analyze Sources.md](2.0 Analyze Sources.md) | 2.0 Analyze Sources — flows `A01`–`A26` |
| [3.0 Lower and Validate IR.md](3.0 Lower and Validate IR.md) | 3.0 Lower and Validate IR — flows `L01`–`L13` |
| [4.0 Interpret IR.md](4.0 Interpret IR.md) | 4.0 Interpret IR — flows `I01`–`I13` |
| [5.0 Compile IR.md](5.0 Compile IR.md) | 5.0 Compile IR — flows `G01`–`G17` |
| [6.0 Record process log.md](6.0 Record process log.md) | 6.0 Record process log — flows `R01`–`R12` |

Each file follows: Diagram → Data flow (INPUT/OUTPUT) → Process description → Flow catalog → Subprocesses → Stores → Associated modules (old architecture).
