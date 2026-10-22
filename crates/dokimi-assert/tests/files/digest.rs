// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `files/digest`: the SHA-256 digest that a record states for a
//! file whose content is longer than 65,536 bytes.
//!
//! The digests are those of Python's `hashlib.sha256` for the same bytes.

use crate::helpers::{records, written};
use dokimi_assert::Recorder;
use dokimi_assert::files::{self, Tree};

/// Returns `size` bytes that take every value of a byte.
fn content(size: usize) -> Vec<u8> {
    (0..size)
        .map(|at| u8::try_from((at * 7 + 3) % 256).expect("a remainder of 256 is a byte"))
        .collect()
}

mod sha256 {
    use super::*;

    #[test]
    fn states_the_digest_of_each_length_of_the_last_block() {
        struct Case {
            name: &'static str,
            give_size: usize,
            want_digest: &'static str,
        }
        let tests = [
            Case {
                name: "one byte past a block",
                give_size: 65_537,
                want_digest: "sha256:ad8b370d36508e55e3c9cd44667a6e36e35955d0ff9f8fe59805bb18c2db5dd8",
            },
            Case {
                name: "the longest tail that its length follows in one block",
                give_size: 65_591,
                want_digest: "sha256:195e550b48925b74af2b904948010f60a4673687cf6a41ce769c10b1c1b94cba",
            },
            Case {
                name: "the shortest tail that needs a second block",
                give_size: 65_592,
                want_digest: "sha256:2af4614e759ccd73e6ebff159bcc2e1e3bf3c188e4ec12e4846625942f9c7be2",
            },
            Case {
                name: "one byte short of a block",
                give_size: 65_599,
                want_digest: "sha256:5f8e1c35e118a967b1173d0c0403f861ee2abbc16b751dc54713efb2fbf728c5",
            },
            Case {
                name: "whole blocks",
                give_size: 65_600,
                want_digest: "sha256:bd6fd9e01431e04b46203f36f0b66a2db1e26494526f0e3b48d62f38c6c340a6",
            },
        ];
        for tt in tests {
            let dir = written(&Tree::from([(
                "large.bin",
                files::bytes(content(tt.give_size)),
            )]));
            let seat = Recorder::new();
            files::equal(&seat, &dir, &Tree::new(), "c");
            let got = &records(&seat)[0]["detail"]["got"]["entries"][0];
            assert_eq!(got["digest"], tt.want_digest, "{}", tt.name);
            assert_eq!(got["size"], tt.give_size, "{}", tt.name);
        }
    }
}
