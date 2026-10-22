// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::engine::token`, the replay token of a case.

use dokimi_assert::prop::engine::choice::Choice;
use dokimi_assert::prop::engine::token::{PREFIX, decode, encode};

/// Returns the reason why `token` does not decode, after the token.
fn refusal(token: &str) -> String {
    let refused = decode(token).expect_err("the text is no token");
    refused
        .to_string()
        .replacen(&format!("prop: token {token:?} "), "", 1)
}

mod encode {
    use super::*;

    #[test]
    fn round_trips_every_kind_of_choice() {
        let choices = vec![
            Choice::Integer(-3),
            Choice::Integer(300),
            Choice::Float(0.5),
            Choice::Float(f64::NAN),
            Choice::Sequence(vec![1, 200, 0]),
        ];
        let token = encode(&choices);
        assert!(token.starts_with(PREFIX), "{token}");
        let decoded = decode(&token).expect("the token decodes");
        assert_eq!(decoded.len(), 5);
        assert_eq!(decoded[..3], choices[..3]);
        assert!(decoded[3].as_float().is_some_and(f64::is_nan));
        assert_eq!(decoded[4], choices[4]);
    }
}

mod decode {
    use super::*;

    #[test]
    fn refuses_text_that_is_no_token_of_a_choice_sequence() {
        let tests = [
            ("no prefix", "token", "does not start with prop1:"),
            (
                "a character of one base64 digit",
                "prop1:A",
                "is not base64url",
            ),
            ("an unknown tag", "prop1:BA", "states an unknown tag"),
            ("a cut payload", "prop1:Ag", "ends inside a choice"),
            (
                "a number of 2^64",
                "prop1:AICAgICAgICAgAI",
                "states a number of 2^64 or more",
            ),
            (
                "a number of 2^70",
                "prop1:AICAgICAgICAgIAB",
                "states a number of 2^64 or more",
            ),
            (
                "a negative integer below -2^63",
                "prop1:AYGAgICAgICAgAE",
                "states a negative integer below -2^63",
            ),
            ("a negative zero", "prop1:AQA", "is not in canonical form"),
        ];
        for (name, give, want) in tests {
            assert_eq!(refusal(give), want, "{name}");
        }
    }
}
