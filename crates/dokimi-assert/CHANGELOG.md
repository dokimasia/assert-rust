# dokimi-assert

## 0.3.0

### Minor Changes

- 0523351: Add `stateful::Action::keep` to keep an action in every case without a swarm choice.
- 06313eb: Run the cleanups of the check of `check::rejects` and of each attempt of `eventually` when the body ends.
- 9ebaa1a: **Breaking:** End the call of a property with the fault that ends a call in one of its cases.
- 06313eb: Run the cleanups of a `Collector` when it drops, also while its test panics.
- 2482ef5: Replay a composite in a trace from the entries of its draws, and take its own entry by its label once it decodes.
- 101a7dd: Add `stateful::concurrent_mean` to set the mean number of steps of a concurrent section.
- e528e3a: Replay a draw of `prop::one_of`, `prop::optional`, `prop::list`, `Generator::filter` or `prop::recursive` with a `prop::composite` inside in a trace after the entries of the composite's draws, so its stored case replays its counterexample.
- d281075: Implement definition 12.0.0 and write its version into every call record.
- 3b7713e: Implement definition 12.1.0 and write its version into every call record.
- a011ff6: Implement definition 12.2.0 and write its version into every call record.
- 9ebaa1a: Implement definition 13.0.0 and write its version into every call record.
- 34bc092: Implement definition 13.1.0 and write its version into every call record.
- acb3f71: Implement definition 13.2.0 and write its version into every call record.
- e48816f: Implement definition 13.3.0 and write its version into every call record.
- 06313eb: Implement definition 13.4.0 and write its version into every call record.
- 43169a7: Implement definition 13.5.0 and write its version into every call record.
- 128d4ec: Implement definition 13.6.0 and write its version into every call record.
- 7b1cf4f: Implement definition 14.0.0 and write its version into every call record.
- 7dba211: Implement definition 14.1.0 and write its version into every call record.
- 101a7dd: Implement definition 14.2.0 and write its version into every call record.
- eea35f0: Implement definition 15.0.0 and write its version into every call record.
- 42abb36: Implement definition 16.0.0 and write its version into every call record.
- 4f9749a: Implement definition 16.1.0 and write its version into every call record.
- 0523351: Implement definition 16.2.0 and write its version into every call record.
- 481b335: Implement definition 16.3.0 and write its version into every call record.
- 2482ef5: Implement definition 16.4.0 and write its version into every call record.
- e528e3a: Implement definition 16.5.0 and write its version into every call record.
- 7b1cf4f: **Breaking:** Skip each configuration of the search of `history::is_linearizable` that its memo covers.
- eea35f0: **Breaking:** Report a lost update in `history::is_serializable` and `history::has_snapshot_isolation`.
- acb3f71: Add `max_live_bytes` to `check` and `soft`, a ceiling on the bytes that a call keeps allocated at each call of a probe.
- 42abb36: **Breaking:** Add `stateful::max_releases` and `Strategy::max_releases` to fail a case at the limit of releases of one run or one advance.
- 34bc092: Add `Call` and `Seat::observe_call`, so a seat observes every assertion call, a passing one included.
- 7b1cf4f: **Breaking:** Add `prop::confirm` to replay every n-th valid case of a run from its choices, and the variant `Phase::Confirm` of its calls.
- 128d4ec: Add `prop::on_result` and `prop::RunResult` to hand a function the result of each run of a property.
- 42abb36: Add `prop::replays` to replay a failing case n times before the shrink.
- 43169a7: Add `prop::run_seed` to return the seed that a run takes without `prop::seed`.
- 06313eb: Run the cleanups that a `Recorder` still keeps when it drops.
- 06313eb: Add `Recorder::run_cleanups` to run the cleanups that a recorder keeps.
- d281075: **Breaking:** Decode each value of a type with the position generator inside the extension of a stated `prop::recursive` generator of the type.
- e48816f: Check histories of read-write register transactions with `history::is_serializable` and `history::has_snapshot_isolation`.
- 3b7713e: End the call of `check::rejects` as skipped when the check skips before it fails.
- 42abb36: Write the replays of a failing case and the replays that reproduced it into the fields `replays` and `reproduced` of the detail and of `prop::RunResult`.
- 42abb36: Add `stateful::Scheduler::advance` to move the clock of a scheduler deadline by deadline.
- 42abb36: Add `stateful::Scheduler::clock` and `stateful::TaskClock` to park a task until a deadline of the clock of its scheduler.
- 42abb36: **Breaking:** Name `run()` and `advance()` in the panic of a task that calls its own scheduler.
- 42abb36: **Breaking:** Make `stateful::Scheduler::spawn` take a function that receives a handle of the scheduler and returns a `Send` future.
- 42abb36: Add `stateful::Scheduler::wait` to park a task until a condition returns true.
- 06313eb: Add `Seat::cleanup`, whose default ends the call with a fault.
- 3b7713e: Add `Seat::skip`, whose default writes the reason to the standard error, and pass the skip of a property to its seat through it.
- 3b7713e: Write the reason of every skip into the field `reason` of its call record.
- 481b335: Replay the recorded counterexample of a stored case whose choices decode to other values than its entry records.
- 481b335: Write the steps of a machine among the draws of a stored case in entries of format 2.
- 43169a7: Add `write_record` to write a record of another kind to the artifact of its kind beside the test binary.

### Patch Changes

- 06313eb: Fix `prop::Case` to stop its cancellation handle before its first cleanup runs.
- e528e3a: Fix a trace to replay a composite that `prop::using` states for a type from the entries of its draws, also when the entry of the composite does not record a value.

## 0.2.0

### Minor Changes

- c87999e: Add `prop::Case::skip` to end the run of a property as skipped.
- c87999e: Implement definition 11.0.0 and write its version into every call record.
