use dokimi_assert::prop::Arbitrary;

#[derive(Arbitrary)]
struct Order {
    #[prop(minimum = 1)]
    quantity: u32,
}

fn main() {}
