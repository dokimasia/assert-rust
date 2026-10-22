use dokimi_assert::prop::Arbitrary;

#[derive(Arbitrary)]
enum Payment {
    #[prop(min = 1)]
    Paid(i64),
}

fn main() {}
