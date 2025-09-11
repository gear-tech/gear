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

//! This module provides type for string with statically limited length.

use alloc::{borrow::Cow, string::String};
use derive_more::{AsRef, Deref, Display, Into};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

/// Wrapped string to fit [`Self::MAX_LEN`] amount of bytes.
///
/// The [`Cow`] is used to avoid allocating a new `String` when
/// the `LimitedStr` is created from a `&str`.
///
/// Plain `str` is not used because it can't be properly
/// encoded/decoded via scale codec.
#[derive(
    Debug,
    Display,
    Clone,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Decode,
    Encode,
    Hash,
    TypeInfo,
    AsRef,
    Deref,
    Into,
)]
#[as_ref(forward)]
#[deref(forward)]
pub struct LimitedStr<'a>(Cow<'a, str>);

impl<'a> LimitedStr<'a> {
    /// Maximum length of the string.
    pub const MAX_LEN: usize = 1024;

    /// Calculates safe truncation position to truncate
    /// a string down to [`Self::MAX_LEN`] bytes or less.
    fn cut_index(s: &str, pos: usize) -> usize {
        (0..=pos.min(s.len()))
            .rev()
            .find(|&pos| s.is_char_boundary(pos))
            .unwrap_or(0)
    }

    /// Constructs a limited string from a string.
    ///
    /// Checks the size of the string.
    pub fn try_new<S: Into<Cow<'a, str>>>(s: S) -> Result<Self, LimitedStrError> {
        let s = s.into();

        if s.len() > Self::MAX_LEN {
            Err(LimitedStrError)
        } else {
            Ok(Self(s))
        }
    }

    /// Constructs a limited string from a `&str`
    /// truncating it if it's too long.
    pub fn truncated(s: &'a str) -> Self {
        Self(s[..Self::cut_index(s, Self::MAX_LEN)].into())
    }

    /// Constructs a limited string from a [`String`]
    /// truncating it if it's too long.
    pub fn owned_truncated(mut s: String) -> Self {
        s.truncate(Self::cut_index(&s, Self::MAX_LEN));
        Self(s.into())
    }

    /// Constructs a limited string from a static
    /// string literal small enough to fit the limit.
    ///
    /// Should be used only with static string literals.
    /// In that case it can check the string length
    /// in compile time.
    ///
    /// # Panics
    ///
    /// Can panic in runtime if the passed string is
    /// not a static string literal and is too long.
    #[track_caller]
    pub const fn from_small_str(s: &'static str) -> Self {
        if s.len() > Self::MAX_LEN {
            panic!("{}", LimitedStrError::MESSAGE)
        }

        Self(Cow::Borrowed(s))
    }

    /// Return string slice.
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }

    /// Return inner value.
    pub fn into_inner(self) -> Cow<'a, str> {
        self.0
    }
}

impl<'a> TryFrom<&'a str> for LimitedStr<'a> {
    type Error = LimitedStrError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl<'a> TryFrom<String> for LimitedStr<'a> {
    type Error = LimitedStrError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

/// The error type returned when a conversion from `&str` to [`LimitedStr`] fails.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Display)]
#[display("string must be less than {} bytes", LimitedStr::MAX_LEN)]
pub struct LimitedStrError;

impl LimitedStrError {
    /// Static error message.
    pub const MESSAGE: &str = "string must not be longer than `LimitedStr::MAX_LEN` bytes";

    /// Converts the error into a static error message.
    pub const fn as_str(&self) -> &'static str {
        Self::MESSAGE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, distributions::Standard};

    fn assert_result(string: &'static str, max_bytes: usize, expectation: &'static str) {
        let string = &string[..LimitedStr::cut_index(string, max_bytes)];
        assert_eq!(string, expectation);
    }

    fn check_panicking(initial_string: &'static str, upper_boundary: usize) {
        let initial_size = initial_string.len();

        for max_bytes in 0..=upper_boundary {
            let string = &initial_string[..LimitedStr::cut_index(initial_string, max_bytes)];

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
                .collect::<String>();
            string.truncate(LimitedStr::cut_index(&string, max_bytes));

            if string.len() > max_bytes {
                panic!("String '{}' input invalidated algorithms property", string);
            }
        }
    }
}
