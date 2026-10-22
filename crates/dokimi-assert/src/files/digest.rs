//! The SHA-256 digest of FIPS 180-4, which a record states for a file whose
//! content is too long to state in full.

use std::fmt::Write as _;

/// The initial hash value: the first 32 bits of the fractional parts of the
/// square roots of the first 8 primes.
const INITIAL: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// The round constants: the first 32 bits of the fractional parts of the
/// cube roots of the first 64 primes.
const ROUNDS: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// Returns the digest of `content` as a record states it: `sha256:` and 64
/// lowercase hexadecimal digits.
pub(crate) fn sha256(content: &[u8]) -> String {
    let mut state = INITIAL;
    let bits = u64::try_from(content.len())
        .unwrap_or(u64::MAX)
        .wrapping_mul(8);
    let mut tail = content.chunks_exact(64).remainder().to_vec();
    tail.push(0x80);
    while tail.len() % 64 != 56 {
        tail.push(0);
    }
    tail.extend_from_slice(&bits.to_be_bytes());
    for block in content.chunks_exact(64).chain(tail.chunks_exact(64)) {
        compress(&mut state, block);
    }
    state
        .iter()
        .fold(String::from("sha256:"), |mut text, word| {
            // Writing to a String cannot fail.
            let _ = write!(text, "{word:08x}");
            text
        })
}

/// Folds one block of 64 bytes into `state`.
fn compress(state: &mut [u32; 8], block: &[u8]) {
    let mut schedule = [0_u32; 64];
    for (word, bytes) in schedule.iter_mut().zip(block.chunks_exact(4)) {
        *word = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    }
    for t in 16..64 {
        let s0 = schedule[t - 15].rotate_right(7)
            ^ schedule[t - 15].rotate_right(18)
            ^ (schedule[t - 15] >> 3);
        let s1 = schedule[t - 2].rotate_right(17)
            ^ schedule[t - 2].rotate_right(19)
            ^ (schedule[t - 2] >> 10);
        schedule[t] = schedule[t - 16]
            .wrapping_add(s0)
            .wrapping_add(schedule[t - 7])
            .wrapping_add(s1);
    }
    // The working variables a to h of FIPS 180-4, at the indices 0 to 7.
    let mut vars = *state;
    for (round, word) in ROUNDS.iter().zip(schedule) {
        let s1 = vars[4].rotate_right(6) ^ vars[4].rotate_right(11) ^ vars[4].rotate_right(25);
        let choice = (vars[4] & vars[5]) ^ (!vars[4] & vars[6]);
        let first = vars[7]
            .wrapping_add(s1)
            .wrapping_add(choice)
            .wrapping_add(*round)
            .wrapping_add(word);
        let s0 = vars[0].rotate_right(2) ^ vars[0].rotate_right(13) ^ vars[0].rotate_right(22);
        let majority = (vars[0] & vars[1]) ^ (vars[0] & vars[2]) ^ (vars[1] & vars[2]);
        // Each value moves to the next index, so b takes the value of a and
        // e the value of d. Then a and e take their new values.
        vars.rotate_right(1);
        vars[0] = first.wrapping_add(s0.wrapping_add(majority));
        vars[4] = vars[4].wrapping_add(first);
    }
    for (word, added) in state.iter_mut().zip(vars) {
        *word = word.wrapping_add(added);
    }
}
