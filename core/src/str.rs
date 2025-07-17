// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! String with limited length implementation

use alloc::{borrow::Cow, string::String};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

/// Max amount of bytes allowed to be thrown as string explanation of the error.
pub const TRIMMED_MAX_LEN: usize = 1024;

fn smart_truncate(s: &mut String, max_bytes: usize) {
    let mut last_byte = max_bytes;

    if s.len() > last_byte {
        while !s.is_char_boundary(last_byte) {
            last_byte = last_byte.saturating_sub(1);
        }

        s.truncate(last_byte);
    }
}

/// Wrapped string to fit [`TRIMMED_MAX_LEN`] amount of bytes.
///
/// The `Cow` is used to avoid allocating a new `String` when the `LimitedStr` is
/// created from a `&str`.
///
/// Plain `str` is not used because it can't be properly encoded/decoded via scale codec.
#[derive(
    TypeInfo, Encode, Decode, Debug, Clone, derive_more::Display, PartialEq, Eq, PartialOrd, Ord,
)]
pub struct LimitedStr<'a>(Cow<'a, str>);

impl<'a> LimitedStr<'a> {
    const INIT_ERROR_MSG: &'static str = concat!(
        "String must be less than ",
        stringify!(TRIMMED_MAX_LEN),
        " bytes."
    );

    /// Convert from `&str` in compile-time.
    #[track_caller]
    pub const fn from_small_str(s: &'a str) -> Self {
        if s.len() > TRIMMED_MAX_LEN {
            panic!("{}", Self::INIT_ERROR_MSG)
        }

        Self(Cow::Borrowed(s))
    }

    /// Return string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    /// Return inner value.
    pub fn into_inner(self) -> Cow<'a, str> {
        self.0
    }
}

/// The error type returned when a conversion from `&str` to [`LimitedStr`] fails.
#[derive(Clone, Debug, derive_more::Display)]
#[display("String must be less than {TRIMMED_MAX_LEN} bytes")]
pub struct LimitedStrTryFromError;

impl<'a> TryFrom<&'a str> for LimitedStr<'a> {
    type Error = LimitedStrTryFromError;

    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        if s.len() > TRIMMED_MAX_LEN {
            return Err(LimitedStrTryFromError);
        }

        Ok(Self(Cow::from(s)))
    }
}

impl From<String> for LimitedStr<'_> {
    fn from(mut s: String) -> Self {
        smart_truncate(&mut s, TRIMMED_MAX_LEN);
        Self(Cow::from(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, distributions::Standard};

    fn assert_result(string: &'static str, max_bytes: usize, expectation: &'static str) {
        let mut string = string.into();
        smart_truncate(&mut string, max_bytes);
        assert_eq!(string, expectation);
    }

    fn check_panicking(initial_string: &'static str, upper_boundary: usize) {
        let initial_size = initial_string.len();

        for max_bytes in 0..=upper_boundary {
            let mut string = initial_string.into();
            smart_truncate(&mut string, max_bytes);

            // Extra check just for confidence.
            if max_bytes >= initial_size {
                assert_eq!(string, initial_string);
            }
        }
    }

    #[test]
    fn truncate_test() {
        // String for demonstration with UTF_8 encoding.
        let utf_8 = "hello";
        // Length in bytes.
        assert_eq!(utf_8.len(), 5);
        // Length in chars.
        assert_eq!(utf_8.chars().count(), 5);

        // Check that `smart_truncate` never panics.
        //
        // It calls the `smart_truncate` with `max_bytes` arg in 0..= len * 2.
        check_panicking(utf_8, utf_8.len().saturating_mul(2));

        // Asserting results.
        assert_result(utf_8, 0, "");
        assert_result(utf_8, 1, "h");
        assert_result(utf_8, 2, "he");
        assert_result(utf_8, 3, "hel");
        assert_result(utf_8, 4, "hell");
        assert_result(utf_8, 5, "hello");
        assert_result(utf_8, 6, "hello");

        // String for demonstration with CJK encoding.
        let cjk = "你好吗";
        // Length in bytes.
        assert_eq!(cjk.len(), 9);
        // Length in chars.
        assert_eq!(cjk.chars().count(), 3);

        // Check that `smart_truncate` never panics.
        //
        // It calls the `smart_truncate` with `max_bytes` arg in 0..= len * 2.
        check_panicking(cjk, cjk.len().saturating_mul(2));

        // Asserting results.
        assert_result(cjk, 0, "");
        assert_result(cjk, 1, "");
        assert_result(cjk, 2, "");
        assert_result(cjk, 3, "你");
        assert_result(cjk, 4, "你");
        assert_result(cjk, 5, "你");
        assert_result(cjk, 6, "你好");
        assert_result(cjk, 7, "你好");
        assert_result(cjk, 8, "你好");
        assert_result(cjk, 9, "你好吗");
        assert_result(cjk, 10, "你好吗");

        // String for demonstration with mixed CJK and UTF-8 encoding.
        // Chaotic sum of "hello" and "你好吗".
        // Length in bytes.
        let mix = "你he好l吗lo";
        assert_eq!(mix.len(), utf_8.len() + cjk.len());
        assert_eq!(mix.len(), 14);
        // Length in chars.
        assert_eq!(
            mix.chars().count(),
            utf_8.chars().count() + cjk.chars().count()
        );
        assert_eq!(mix.chars().count(), 8);

        // Check that `smart_truncate` never panics.
        //
        // It calls the `smart_truncate` with `max_bytes` arg in 0..= len * 2.
        check_panicking(mix, mix.len().saturating_mul(2));

        // Asserting results.
        assert_result(mix, 0, "");
        assert_result(mix, 1, "");
        assert_result(mix, 2, "");
        assert_result(mix, 3, "你");
        assert_result(mix, 4, "你h");
        assert_result(mix, 5, "你he");
        assert_result(mix, 6, "你he");
        assert_result(mix, 7, "你he");
        assert_result(mix, 8, "你he好");
        assert_result(mix, 9, "你he好l");
        assert_result(mix, 10, "你he好l");
        assert_result(mix, 11, "你he好l");
        assert_result(mix, 12, "你he好l吗");
        assert_result(mix, 13, "你he好l吗l");
        assert_result(mix, 14, "你he好l吗lo");
        assert_result(mix, 15, "你he好l吗lo");
    }

    #[test]
    fn truncate_test_fuzz() {
        for _ in 0..50 {
            let mut thread_rng = rand::thread_rng();

            let rand_len = thread_rng.gen_range(0..=100_000);
            let max_bytes = thread_rng.gen_range(0..=rand_len);
            let mut string = thread_rng
                .sample_iter::<char, _>(Standard)
                .take(rand_len)
                .collect();

            smart_truncate(&mut string, max_bytes);

            if string.len() > max_bytes {
                panic!("String '{}' input invalidated algorithms property", string);
            }
        }
    }
}
