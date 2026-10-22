use dokimi_assert::prop::Arbitrary;

#[derive(Arbitrary)]
struct Order {
    #[prop(rename)]
    quantity: u32,
}

fn main() {}
