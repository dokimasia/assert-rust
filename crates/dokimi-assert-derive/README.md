# dokimi-assert-derive

`#[derive(Arbitrary)]` states the shape of a struct or an enum for the
property forms of [dokimi-assert](https://crates.io/crates/dokimi-assert). A
property form then generates its input from the type alone.

Depend on `dokimi-assert` with its feature `derive`, which re-exports the
macro as `dokimi_assert::prop::Arbitrary`:

```toml
[dev-dependencies]
dokimi-assert = { version = "0.1", features = ["derive"] }
```

```rust
use dokimi_assert::prop;

#[derive(Debug, Clone, PartialEq, prop::Arbitrary)]
struct Order {
    #[prop(min = 1, max = 99)]
    quantity: u32,
    note: Option<String>,
}
```

A struct reads as a record of its fields in declaration order, and an enum as
an enum of its variants. The attribute `#[prop(...)]` states the constraints of
a field with the keys `min`, `max`, `min_size`, `max_size`, `pattern`,
`alphabet`, `allow_nan`, `allow_infinity`, `unit`, `scale` and `version`, and
`rename` names a field or a variant in the shape.
