use dokimi_assert::prop::Arbitrary;

#[derive(Arbitrary)]
struct Order {
    #[prop(pattern = 5)]
    code: String,
}

fn main() {}
