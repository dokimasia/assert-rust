---
"dokimi-assert": minor
---

**Breaking:** Make `stateful::Scheduler::spawn` take a function that receives a handle of the scheduler and returns a `Send` future.
