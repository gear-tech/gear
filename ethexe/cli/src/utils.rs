// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use core::{fmt, marker::PhantomData, str};

pub trait Currency {
    const SYMBOL: &'static str;
    const DECIMALS: u32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ethereum;

impl Currency for Ethereum {
    const SYMBOL: &'static str = "ETH";
    const DECIMALS: u32 = 18;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrappedVara;

impl Currency for WrappedVara {
    const SYMBOL: &'static str = "WVARA";
    const DECIMALS: u32 = 12;
}

const TEN: u128 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FormattedValue<C: Currency> {
    value: u128,
    phantom: PhantomData<C>,
}

impl<C: Currency> FormattedValue<C> {
    #[inline]
    pub const fn new(value: u128) -> Self {
        FormattedValue {
            value,
            phantom: PhantomData,
        }
    }

    #[inline]
    pub const fn into_inner(self) -> u128 {
        self.value
    }
}

impl<C: Currency> fmt::Display for FormattedValue<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = self.value;
        let unit = TEN.pow(C::DECIMALS);

        let int_part = value / unit;
        let frac_part = value % unit;

        let symbol = C::SYMBOL;

        if frac_part == 0 {
            return write!(f, "{int_part} {symbol}");
        }

        let frac_part = format!("{frac_part:0width$}", width = C::DECIMALS as _)
            .trim_end_matches('0')
            .to_string();

        write!(f, "{int_part}.{frac_part} {symbol}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParseFormattedValueError {
    #[error("Expected number and currency symbol \"{expected_currency}\", separated by space")]
    InvalidFormat { expected_currency: &'static str },
    #[error("Failed to parse u128 integer")]
    IntParse,
    #[error("Integer overflow occurred during parsing")]
    IntOverflow,
}

impl<C: Currency> str::FromStr for FormattedValue<C> {
    type Err = ParseFormattedValueError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut it = s.split_ascii_whitespace();
        let s = match (it.next(), it.next(), it.next()) {
            (Some(s), Some(currency), None) if currency == C::SYMBOL => s,
            _ => {
                return Err(Self::Err::InvalidFormat {
                    expected_currency: C::SYMBOL,
                });
            }
        };

        let mut it = s.split('.');
        let (int_part, frac_part) = match (it.next(), it.next(), it.next()) {
            (Some(int_part_str), None, None) => {
                let int_part = int_part_str
                    .parse::<u128>()
                    .map_err(|_| Self::Err::IntParse)?;
                (int_part, None)
            }
            (Some(int_part_str), Some(frac_part_str), None)
                if frac_part_str.len() <= C::DECIMALS as _ =>
            {
                let int_part = int_part_str
                    .parse::<u128>()
                    .map_err(|_| Self::Err::IntParse)?;
                let frac_part = frac_part_str
                    .parse::<u128>()
                    .map_err(|_| Self::Err::IntParse)?;
                (int_part, Some((frac_part, frac_part_str.len() as u32)))
            }
            _ => {
                return Err(Self::Err::InvalidFormat {
                    expected_currency: C::SYMBOL,
                });
            }
        };

        let frac_scaled = if let Some((frac_part, frac_part_len)) = frac_part {
            let scale = TEN.pow(C::DECIMALS - frac_part_len);
            frac_part.checked_mul(scale).ok_or(Self::Err::IntOverflow)?
        } else {
            0
        };

        let unit = TEN.pow(C::DECIMALS);
        let value = int_part
            .checked_mul(unit)
            .and_then(|v| v.checked_add(frac_scaled))
            .ok_or(Self::Err::IntOverflow)?;

        Ok(FormattedValue::new(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RawOrFormattedValue<C: Currency> {
    Raw(u128),
    Formatted(FormattedValue<C>),
}

impl<C: Currency> RawOrFormattedValue<C> {
    pub fn into_inner(self) -> u128 {
        match self {
            Self::Raw(value) => value,
            Self::Formatted(formatted) => formatted.into_inner(),
        }
    }
}

impl<C: Currency> fmt::Display for RawOrFormattedValue<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Raw(value) => write!(f, "{value}"),
            Self::Formatted(formatted) => write!(f, "{formatted}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParseRawOrFormattedValue {
    #[error("Invalid format")]
    InvalidFormat,
    #[error("Failed to parse u128 integer")]
    IntParse,
    #[error("Failed to parse formatted value: {0}")]
    FormattedParseError(#[from] ParseFormattedValueError),
}

impl<C: Currency> str::FromStr for RawOrFormattedValue<C> {
    type Err = ParseRawOrFormattedValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut it = s.split_ascii_whitespace();
        match (it.next(), it.next(), it.next()) {
            (Some(value_str), None, None) => {
                let value = value_str.parse::<u128>().map_err(|_| Self::Err::IntParse)?;
                Ok(Self::Raw(value))
            }
            (Some(_), Some(_), None) => {
                let formatted = s.parse::<FormattedValue<C>>()?;
                Ok(Self::Formatted(formatted))
            }
            _ => Err(Self::Err::InvalidFormat),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formatted_value() {
        const ETHER: u128 = TEN.pow(18);

        for (value, expected) in [
            (1, "0.000000000000000001 ETH"),
            (10, "0.00000000000000001 ETH"),
            (100, "0.0000000000000001 ETH"),
            (1000, "0.000000000000001 ETH"),
            (10000, "0.00000000000001 ETH"),
            (100000, "0.0000000000001 ETH"),
            (1000000, "0.000000000001 ETH"),
            (10000000, "0.00000000001 ETH"),
            (100000000, "0.0000000001 ETH"),
            (1000000000, "0.000000001 ETH"),
            (10000000000, "0.00000001 ETH"),
            (100000000000, "0.0000001 ETH"),
            (1000000000000, "0.000001 ETH"),
            (10000000000000, "0.00001 ETH"),
            (100000000000000, "0.0001 ETH"),
            (1000000000000000, "0.001 ETH"),
            (10000000000000000, "0.01 ETH"),
            (100000000000000000, "0.1 ETH"),
            (ETHER + 1, "1.000000000000000001 ETH"),
            (ETHER + 10, "1.00000000000000001 ETH"),
            (ETHER + 100, "1.0000000000000001 ETH"),
            (ETHER + 1000, "1.000000000000001 ETH"),
            (ETHER + 10000, "1.00000000000001 ETH"),
            (ETHER + 100000, "1.0000000000001 ETH"),
            (ETHER + 1000000, "1.000000000001 ETH"),
            (ETHER + 10000000, "1.00000000001 ETH"),
            (ETHER + 100000000, "1.0000000001 ETH"),
            (ETHER + 1000000000, "1.000000001 ETH"),
            (ETHER + 10000000000, "1.00000001 ETH"),
            (ETHER + 100000000000, "1.0000001 ETH"),
            (ETHER + 1000000000000, "1.000001 ETH"),
            (ETHER + 10000000000000, "1.00001 ETH"),
            (ETHER + 100000000000000, "1.0001 ETH"),
            (ETHER + 1000000000000000, "1.001 ETH"),
            (ETHER + 10000000000000000, "1.01 ETH"),
            (ETHER + 100000000000000000, "1.1 ETH"),
            (ETHER, "1 ETH"),
            (2 * ETHER, "2 ETH"),
            (3 * ETHER, "3 ETH"),
            (42 * ETHER, "42 ETH"),
            (100 * ETHER, "100 ETH"),
        ] {
            let value = FormattedValue::<Ethereum>::new(value);
            assert_eq!(format!("{value}"), expected);
            assert_eq!(expected.parse::<FormattedValue<Ethereum>>().unwrap(), value);
        }
    }
}
