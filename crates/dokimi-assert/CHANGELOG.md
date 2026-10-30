# dokimi-assert

## 0.3.0

### Minor Changes

- 06313eb: Run the cleanups of the check of `check::rejects` and of each attempt of `eventually` when the body ends.
- 9ebaa1a: **Breaking:** End the call of a property with the fault that ends a call in one of its cases.
- 06313eb: Run the cleanups of a `Collector` when it drops, also while its test panics.
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
- acb3f71: Add `max_live_bytes` to `check` and `soft`, a ceiling on the bytes that a call keeps allocated at each call of a probe.
- 34bc092: Add `Call` and `Seat::observe_call`, so a seat observes every assertion call, a passing one included.
- 128d4ec: Add `prop::on_result` and `prop::RunResult` to hand a function the result of each run of a property.
- 43169a7: Add `prop::run_seed` to return the seed that a run takes without `prop::seed`.
- 06313eb: Run the cleanups that a `Recorder` still keeps when it drops.
- 06313eb: Add `Recorder::run_cleanups` to run the cleanups that a recorder keeps.
- d281075: **Breaking:** Decode each value of a type with the position generator inside the extension of a stated `prop::recursive` generator of the type.
- e48816f: Check histories of read-write register transactions with `history::is_serializable` and `history::has_snapshot_isolation`.
- 3b7713e: End the call of `check::rejects` as skipped when the check skips before it fails.
- 06313eb: Add `Seat::cleanup`, whose default ends the call with a fault.
- 3b7713e: Add `Seat::skip`, whose default writes the reason to the standard error, and pass the skip of a property to its seat through it.
- 3b7713e: Write the reason of every skip into the field `reason` of its call record.
- 43169a7: Add `write_record` to write a record of another kind to the artifact of its kind beside the test binary.

### Patch Changes

- 06313eb: Fix `prop::Case` to stop its cancellation handle before its first cleanup runs.

## 0.2.0

### Minor Changes

- c87999e: Add `prop::Case::skip` to end the run of a property as skipped.
- c87999e: Implement definition 11.0.0 and write its version into every call record.
