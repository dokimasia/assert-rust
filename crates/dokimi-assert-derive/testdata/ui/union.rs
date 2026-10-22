use dokimi_assert::prop::Arbitrary;

#[derive(Arbitrary)]
union Bits {
    whole: u32,
    float: f32,
}

fn main() {}
