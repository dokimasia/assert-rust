use dokimi_assert::prop::Arbitrary;

#[derive(Arbitrary)]
#[prop(rename = "order")]
struct Order {
    quantity: u32,
}

fn main() {}
