// CurveAmm Dex
// Implementation based on https://github.com/equilibrium-eosdt/equilibrium-curve-amm/blob/master/pallets/equilibrium-curve-amm/src/lib.rs
// For more details read
//      https://miguelmota.com/blog/understanding-stableswap-curve/
//      https://curve.fi/files/stableswap-paper.pdf
//      https://github.com/equilibrium-eosdt/equilibrium-curve-amm/blob/master/docs/deducing-get_y-formulas.pdf

#![no_std]
#![feature(const_btree_new)]

use codec::{Decode, Encode};
use gstd::{debug, exec, msg, prelude::*, ProgramId};
use primitive_types::H256;
use scale_info::TypeInfo;
use sp_arithmetic::fixed_point::FixedU128;
use sp_arithmetic::per_things::Permill;
use sp_arithmetic::traits::CheckedAdd;
use sp_arithmetic::traits::CheckedDiv;
use sp_arithmetic::traits::CheckedMul;
use sp_arithmetic::traits::CheckedSub;
use sp_arithmetic::traits::One;
use sp_arithmetic::traits::Zero;
use sp_arithmetic::FixedPointNumber;

const GAS_RESERVE: u64 = 500_000_000;

/// Type that represents index type of token in the pool passed from the outside as an extrinsic
/// argument.
pub type PoolTokenIndex = u32;

/// Type that represents pool id
pub type PoolId = u32;

/// Type that represents asset id
pub type AssetId = u32;



/// Storage record type for a pool
pub struct PoolInfo {
    /// Owner of pool
    pub owner: ProgramId,
    /// LP multiasset
    pub pool_asset: AssetId,
    /// List of multiassets supported by the pool
    pub assets: Vec<AssetId>,
    /// Initial amplification coefficient (leverage)
    pub amplification: FixedU128,
    /// Amount of the fee pool charges for the exchange
    pub fee: Permill,
    /// Amount of the admin fee pool charges for the exchange
    pub admin_fee: Permill,
    /// Current balances excluding admin_fee
    pub balances: Vec<FixedU128>,
    /// Current balances including admin_fee
    pub total_balances: Vec<FixedU128>,
}

struct CurveAmm {
    /// Current number of pools (also ID for the next created pool)
    pool_count: PoolId,
    /// Existing pools
    pools: BTreeMap<PoolId, PoolInfo>,
    /// precision to be used in mathematical computations.
    precision: FixedU128,
}

impl CurveAmm {
    /// Find `ann = amp * n^n` where `amp` - amplification coefficient,
    /// `n` - number of coins.
    pub fn get_ann(&self, amp: FixedU128, n: usize) -> Option<FixedU128> {
        let n_coins = FixedU128::saturating_from_integer(n as u128);
        let mut ann = amp;
        for _ in 0..n {
            ann = ann.checked_mul(&n_coins)?;
        }
        Some(ann)
    }

    /// Find `d` preserving StableSwap invariant.
    /// Here `d` - total amount of coins when they have an equal price,
    /// `xp` - coin amounts, `ann` is amplification coefficient multiplied by `n^n`,
    /// where `n` is number of coins.
    ///
    /// # Notes
    ///
    /// D invariant calculation in non-overflowing integer operations iteratively
    ///
    /// ```pseudocode
    ///  A * sum(x_i) * n^n + D = A * D * n^n + D^(n+1) / (n^n * prod(x_i))
    /// ```
    ///
    /// Converging solution:
    ///
    /// ```pseudocode
    /// D[j + 1] = (A * n^n * sum(x_i) - D[j]^(n+1) / (n^n * prod(x_i))) / (A * n^n - 1)
    /// ```
    pub fn get_d(&self, xp_f: &[FixedU128], ann_f: FixedU128) -> Option<FixedU128> {
        let zero = FixedU128::zero();
        let one = FixedU128::one();
        let n = FixedU128::saturating_from_integer(u128::try_from(xp_f.len()).ok()?);
        let sum = xp_f.iter().try_fold(zero, |s, x| s.checked_add(x))?;
        if sum == zero {
            return Some(zero);
        }
        let mut d = sum;

        for _ in 0..255 {
            let mut d_p = d;
            for x in xp_f.iter() {
                // d_p = d_p * d / (x * n)
                d_p = d_p.checked_mul(&d)?.checked_div(&x.checked_mul(&n)?)?;
            }
            let d_prev = d;

            // d = (ann * sum + d_p * n) * d / ((ann - 1) * d + (n + 1) * d_p)
            d = ann_f
                .checked_mul(&sum)?
                .checked_add(&d_p.checked_mul(&n)?)?
                .checked_mul(&d)?
                .checked_div(
                    &ann_f
                        .checked_sub(&one)?
                        .checked_mul(&d)?
                        .checked_add(&n.checked_add(&one)?.checked_mul(&d_p)?)?,
                )?;

            if d > d_prev {
                if d.checked_sub(&d_prev)? <= self.precision {
                    return Some(d);
                }
            } else if d_prev.checked_sub(&d)? <= self.precision {
                return Some(d);
            }
        }
        None
    }
    		/// Here `xp` - coin amounts, `ann` is amplification coefficient multiplied by `n^n`, where
		/// `n` is number of coins.
		///
		/// See https://github.com/equilibrium-eosdt/equilibrium-curve-amm/blob/master/docs/deducing-get_y-formulas.pdf
		/// for detailed explanation about formulas this function uses.
		///
		/// # Notes
		///
		/// Done by solving quadratic equation iteratively.
		///
		/// ```pseudocode
		/// x_1^2 + x_1 * (sum' - (A * n^n - 1) * D / (A * n^n)) = D^(n+1) / (n^2n * prod' * A)
		/// x_1^2 + b * x_1 = c
		///
		/// x_1 = (x_1^2 + c) / (2 * x_1 + b)
		/// ```
		pub fn get_y(
            &self,
			i: usize,
			j: usize,
			x_f: FixedU128,
			xp_f: &[FixedU128],
			ann_f: FixedU128,
		) -> Option<FixedU128> {
			let zero = FixedU128::zero();
			let two = FixedU128::saturating_from_integer(2u8);
			let n = FixedU128::try_from(xp_f.len() as u128).ok()?;

			// Same coin
			if i == j {
				return None
			}
			// j above n
			if j >= xp_f.len() {
				return None
			}
			if i >= xp_f.len() {
				return None
			}
			let d_f = self.get_d(xp_f, ann_f)?;
			let mut c = d_f;
			let mut s = zero;

			// Calculate s and c
			// p is implicitly calculated as part of c
			// note that loop makes n - 1 iterations
			for (k, xp_k) in xp_f.iter().enumerate() {
				let x_k: FixedU128;
				if k == i {
					x_k = x_f;
				} else if k != j {
					x_k = *xp_k;
				} else {
					continue
				}
				// s = s + x_k
				s = s.checked_add(&x_k)?;
				// c = c * d / (x_k * n)
				c = c.checked_mul(&d_f)?.checked_div(&x_k.checked_mul(&n)?)?;
			}
			// c = c * d / (ann * n)
			// At this step we have d^n in the numerator of c
			// and n^(n-1) in its denominator.
			// So we multiplying it by remaining d/n
			c = c.checked_mul(&d_f)?.checked_div(&ann_f.checked_mul(&n)?)?;

			// b = s + d / ann
			// We subtract d later
			let b = s.checked_add(&d_f.checked_div(&ann_f)?)?;
			let mut y = d_f;

			for _ in 0..255 {
				let y_prev = y;
				// y = (y^2 + c) / (2 * y + b - d)
				// Subtract d to calculate b finally
				y = y
					.checked_mul(&y)?
					.checked_add(&c)?
					.checked_div(&two.checked_mul(&y)?.checked_add(&b)?.checked_sub(&d_f)?)?;

				// Equality with the specified precision
				if y > y_prev {
					if y.checked_sub(&y_prev)? <= self.precision {
						return Some(y)
					}
				} else if y_prev.checked_sub(&y)? <= self.precision {
					return Some(y)
				}
			}

			None
		}

		/// Here `xp` - coin amounts, `ann` is amplification coefficient multiplied by `n^n`, where
		/// `n` is number of coins.
		/// Calculate `x[i]` if one reduces `d` from being calculated for `xp` to `d`.
		///
		/// # Notes
		///
		/// Done by solving quadratic equation iteratively.
		///
		/// ```pseudocode
		/// x_1^2 + x_1 * (sum' - (A * n^n - 1) * D / (A * n^n)) = D^(n+1) / (n^2n * prod' * A)
		/// x_1^2 + b * x_1 = c
		///
		/// x_1 = (x_1^2 + c) / (2 * x_1 + b)
		/// ```
		pub fn get_y_d(
            &self,
			i: usize,
			d_f: FixedU128,
			xp_f: &[FixedU128],
			ann_f: FixedU128,
		) -> Option<FixedU128> {
			let zero = FixedU128::zero();
			let two = FixedU128::saturating_from_integer(2u8);
			let n = FixedU128::try_from(xp_f.len() as u128).ok()?;

			if i >= xp_f.len() {
				return None
			}

			let mut c = d_f;
			let mut s = zero;

			for (k, xp_k) in xp_f.iter().enumerate() {
				if k == i {
					continue
				}

				let x = xp_k;

				s = s.checked_add(x)?;
				// c = c * d / (x * n)
				c = c.checked_mul(&d_f)?.checked_div(&x.checked_mul(&n)?)?;
			}
			// c = c * d / (ann * n)
			c = c.checked_mul(&d_f)?.checked_div(&ann_f.checked_mul(&n)?)?;
			// b = s + d / ann
			let b = s.checked_add(&d_f.checked_div(&ann_f)?)?;
			let mut y = d_f;

			for _ in 0..255 {
				let y_prev = y;
				// y = (y*y + c) / (2 * y + b - d)
				y = y
					.checked_mul(&y)?
					.checked_add(&c)?
					.checked_div(&two.checked_mul(&y)?.checked_add(&b)?.checked_sub(&d_f)?)?;

				// Equality with the specified precision
				if y > y_prev {
					if y.checked_sub(&y_prev)? <= self.precision {
						return Some(y)
					}
				} else if y_prev.checked_sub(&y)? <= self.precision {
					return Some(y)
				}
			}

			None
		}
}

// gstd::metadata! {
//     title: "CurveAmm",
//         init:
//             input : InitConfig,
//         handle:
//             input : Action,
//             output : Event,
// }

#[no_mangle]
pub unsafe extern "C" fn handle() {
    // let action: Action = msg::load().expect("Could not load Action");

    // match action {
    // }
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    // let config: InitConfig = msg::load().expect("Unable to decode InitConfig");
    // debug!("FUNGIBLE_TOKEN {:?}", config);
}
