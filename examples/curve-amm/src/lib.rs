// CurveAmm Dex
// Implementation based on https://github.com/equilibrium-eosdt/equilibrium-curve-amm/blob/master/pallets/equilibrium-curve-amm/src/lib.rs
// For more details read
//      https://miguelmota.com/blog/understanding-stableswap-curve/
//      https://curve.fi/files/stableswap-paper.pdf
//      https://github.com/equilibrium-eosdt/equilibrium-curve-amm/blob/master/docs/deducing-get_y-formulas.pdf

#![no_std]
#![feature(const_btree_new)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::collections::BTreeSet;
use alloc::vec;
use alloc::vec::Vec;
use codec::{Decode, Encode};
use fungible_token_messages::{Action, BurnInput, Event, MintInput, TransferData};
use gstd::ActorId;
use gstd::{debug, exec, msg};
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

/// Return Err of the expression: `return Err($expression);`.
///
/// Used as `fail!(expression)`.
#[macro_export]
macro_rules! fail {
    ( $y:expr ) => {{
        return Err($y.into());
    }};
}

/// Evaluate `$x:expr` and if not true return `Err($y:expr)`.
///
/// Used as `ensure!(expression_to_ensure, expression_to_return_on_false)`.
#[macro_export]
macro_rules! ensure {
    ( $x:expr, $y:expr $(,)? ) => {{
        if !$x {
            $crate::fail!($y);
        }
    }};
}

const GAS_RESERVE: u64 = 500_000_000;

#[derive(Debug, Encode, Decode, TypeInfo)]
struct CurveAmmInitConfig {
    /// ownder of the pool
    owner: H256,
    /// FungibleToken program id for Token X.
    x_token_program_id: H256,
    /// FungibleToken program id for Token Y.
    y_token_program_id: H256,
    /// FungibleToken program id for Token LP.
    lp_token_program_id: H256,
    /// amplification_coefficient
    amplification_coefficient: u128,
    /// fee
    fee: u32,
    /// admin fee
    admin_fee: u32,
}

gstd::metadata! {
    title : "CurveAmm",
        init:
            input : CurveAmmInitConfig,
}

// /// Type that represents index type of token in the pool passed from the outside as an extrinsic
// /// argument.
// pub type PoolTokenIndex = u32;

/// Type that represents pool id
pub type PoolId = u32;

// /// Type that represents asset id
// pub type AssetId = u32;

#[derive(Debug)]
pub enum CurveAmmError {
    /// Could not create new asset
    AssetNotCreated,
    /// Values in the storage are inconsistent
    InconsistentStorage,
    /// Not enough assets provided
    NotEnoughAssets,
    /// Some provided assets are not unique
    DuplicateAssets,
    /// Pool with specified id is not found
    PoolNotFound,
    /// Error occurred while performing math calculations
    Math,
    /// Specified asset amount is wrong
    WrongAssetAmount,
    /// Required amount of some token did not reached during adding or removing liquidity
    RequiredAmountNotReached,
    /// Source does not have required amount of coins to complete operation
    InsufficientFunds,
    /// Specified index is out of range
    IndexOutOfRange,
    /// The `AssetChecker` can use this error in case it can't provide better error
    ExternalAssetCheckFailed,
}

/// Storage record type for a pool
#[derive(Debug)]
pub struct PoolInfo {
    /// Owner of pool
    pub owner: ActorId,
    /// LP multiasset
    pub pool_asset: ActorId,
    /// List of multiassets supported by the pool
    pub assets: Vec<ActorId>,
    /// Initial amplification coefficient (leverage)
    pub amplification_coefficient: FixedU128,
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
}

impl CurveAmm {
    #[allow(dead_code)]
    pub fn get_precision(&self) -> FixedU128 {
        FixedU128::saturating_from_rational(1u32, 100_000_000u32)
    }

    /// Find `ann = amp * n^n` where `amp` - amplification coefficient,
    /// `n` - number of coins.
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
                if d.checked_sub(&d_prev)? <= self.get_precision() {
                    return Some(d);
                }
            } else if d_prev.checked_sub(&d)? <= self.get_precision() {
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
            return None;
        }
        // j above n
        if j >= xp_f.len() {
            return None;
        }
        if i >= xp_f.len() {
            return None;
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
                continue;
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
                if y.checked_sub(&y_prev)? <= self.get_precision() {
                    return Some(y);
                }
            } else if y_prev.checked_sub(&y)? <= self.get_precision() {
                return Some(y);
            }
        }

        None
    }

    ///// Here `xp` - coin amounts, `ann` is amplification coefficient multiplied by `n^n`, where
    ///// `n` is number of coins.
    ///// Calculate `x[i]` if one reduces `d` from being calculated for `xp` to `d`.
    /////
    ///// # Notes
    /////
    ///// Done by solving quadratic equation iteratively.
    /////
    ///// ```pseudocode
    ///// x_1^2 + x_1 * (sum' - (A * n^n - 1) * D / (A * n^n)) = D^(n+1) / (n^2n * prod' * A)
    ///// x_1^2 + b * x_1 = c
    /////
    ///// x_1 = (x_1^2 + c) / (2 * x_1 + b)
    ///// ```
    //pub fn get_y_d(
    //    &mut self,
    //    i: usize,
    //    d_f: FixedU128,
    //    xp_f: &[FixedU128],
    //    ann_f: FixedU128,
    //) -> Option<FixedU128> {
    //    let zero = FixedU128::zero();
    //    let two = FixedU128::saturating_from_integer(2u8);
    //    let n = FixedU128::try_from(xp_f.len() as u128).ok()?;

    //    if i >= xp_f.len() {
    //        return None;
    //    }

    //    let mut c = d_f;
    //    let mut s = zero;

    //    for (k, xp_k) in xp_f.iter().enumerate() {
    //        if k == i {
    //            continue;
    //        }

    //        let x = xp_k;

    //        s = s.checked_add(x)?;
    //        // c = c * d / (x * n)
    //        c = c.checked_mul(&d_f)?.checked_div(&x.checked_mul(&n)?)?;
    //    }
    //    // c = c * d / (ann * n)
    //    c = c.checked_mul(&d_f)?.checked_div(&ann_f.checked_mul(&n)?)?;
    //    // b = s + d / ann
    //    let b = s.checked_add(&d_f.checked_div(&ann_f)?)?;
    //    let mut y = d_f;

    //    for _ in 0..255 {
    //        let y_prev = y;
    //        // y = (y*y + c) / (2 * y + b - d)
    //        y = y
    //            .checked_mul(&y)?
    //            .checked_add(&c)?
    //            .checked_div(&two.checked_mul(&y)?.checked_add(&b)?.checked_sub(&d_f)?)?;

    //        // Equality with the specified precision
    //        if y > y_prev {
    //            if y.checked_sub(&y_prev)? <= self.get_precision() {
    //                return Some(y);
    //            }
    //        } else if y_prev.checked_sub(&y)? <= self.get_precision() {
    //            return Some(y);
    //        }
    //    }

    //    None
    //}

    fn create_pool(
        &mut self,
        who: &ActorId,
        assets: Vec<ActorId>,
        pool_asset: &ActorId,
        amplification_coefficient: FixedU128,
        fee: Permill,
        admin_fee: Permill,
    ) -> Result<PoolId, CurveAmmError> {
        // Assets related checks
        ensure!(assets.len() > 1, CurveAmmError::NotEnoughAssets);
        let unique_assets = BTreeSet::<ActorId>::from_iter(assets.iter().copied());
        ensure!(
            unique_assets.len() == assets.len(),
            CurveAmmError::DuplicateAssets
        );

        // Add new pool
        let pool_id = self.pool_count;

        // We expect that PoolInfos have sequential keys.
        // No PoolInfo can have key greater or equal to PoolCount
        ensure!(
            self.pools.get(&pool_id).is_none(),
            CurveAmmError::InconsistentStorage
        );

        let empty_balances = vec![FixedU128::zero(); assets.len()];

        let pool_info = PoolInfo {
            owner: *who,
            pool_asset: *pool_asset,
            assets,
            amplification_coefficient,
            fee,
            admin_fee,
            balances: empty_balances.clone(),
            total_balances: empty_balances,
        };
        self.pools.insert(pool_id, pool_info);

        self.pool_count = pool_id
            .checked_add(1)
            .ok_or(CurveAmmError::InconsistentStorage)?;

        Ok(pool_id)

        // TODO: this needs to be converted to msg::reply
    }

    #[allow(dead_code)]
    async fn add_liquidity(
        &mut self,
        who: &ActorId,
        pool_id: PoolId,
        amounts: Vec<FixedU128>,
        min_mint_amount: FixedU128,
    ) -> Result<(), CurveAmmError> {
        let zero = FixedU128::zero();
        ensure!(
            amounts.iter().all(|&x| x >= zero),
            CurveAmmError::WrongAssetAmount
        );

        let pool = self
            .pools
            .get(&pool_id)
            .ok_or(CurveAmmError::PoolNotFound)?;
        let n_coins = pool.assets.len();
        ensure!(
            n_coins == pool.balances.len(),
            CurveAmmError::InconsistentStorage
        );
        ensure!(n_coins == amounts.len(), CurveAmmError::IndexOutOfRange);
        let ann = self
            .get_ann(pool.amplification_coefficient, n_coins)
            .ok_or(CurveAmmError::Math)?;
        let old_balances = pool.balances.clone();
        let d0 = self.get_d(&old_balances, ann).ok_or(CurveAmmError::Math)?;
        let token_supply;
        let reply: Event =
            msg::send_and_wait_for_reply(pool.pool_asset, &Action::TotalIssuance, 1_000_000_000, 0)
                .await
                .expect("Error in async message");
        // if let Ok(Event::TotalIssuance(balance)) = Event::decode(&mut reply) {
        //     token_supply = FixedU128::saturating_from_integer(balance);
        // } else {
        //     panic!("could not decode TotalIssuance reply");
        // }
        match reply {
            Event::TotalIssuance(bal) => {
                token_supply = FixedU128::saturating_from_integer(bal);
            }
            _ => {
                panic!("could not decode TotalIssuance reply");
            }
        }
        let mut new_balances = old_balances.clone();
        for i in 0..n_coins {
            if token_supply == zero {
                ensure!(amounts[i] > zero, CurveAmmError::WrongAssetAmount);
            }
            new_balances[i] = new_balances[i]
                .checked_add(&amounts[i])
                .ok_or(CurveAmmError::Math)?;
        }
        let d1 = self.get_d(&new_balances, ann).ok_or(CurveAmmError::Math)?;
        ensure!(d1 > d0, CurveAmmError::WrongAssetAmount);
        let mint_amount;
        let mut fees = vec![FixedU128::zero(); n_coins];
        // Only account for fees if we are not the first to deposit
        if token_supply > zero {
            // Deposit x + withdraw y would chargVe about same
            // fees as a swap. Otherwise, one could exchange w/o paying fees.
            // And this formula leads to exactly that equality
            // fee = pool.fee * n_coins / (4 * (n_coins - 1))
            let one = FixedU128::saturating_from_integer(1u8);
            let four = FixedU128::saturating_from_integer(4u8);
            let n_coins_f = FixedU128::saturating_from_integer(n_coins as u128);
            let fee_f: FixedU128 = pool.fee.into();
            let fee_f = fee_f
                .checked_mul(&n_coins_f)
                .ok_or(CurveAmmError::Math)?
                .checked_div(
                    &four
                        .checked_mul(&n_coins_f.checked_sub(&one).ok_or(CurveAmmError::Math)?)
                        .ok_or(CurveAmmError::Math)?,
                )
                .ok_or(CurveAmmError::Math)?;
            let admin_fee_f: FixedU128 = pool.admin_fee.into();
            for i in 0..n_coins {
                // ideal_balance = d1 * old_balances[i] / d0
                let ideal_balance = (|| d1.checked_mul(&old_balances[i])?.checked_div(&d0))()
                    .ok_or(CurveAmmError::Math)?;

                let new_balance = new_balances[i];
                // difference = abs(ideal_balance - new_balance)
                let difference = (if ideal_balance > new_balance {
                    ideal_balance.checked_sub(&new_balance)
                } else {
                    new_balance.checked_sub(&ideal_balance)
                })
                .ok_or(CurveAmmError::Math)?;

                fees[i] = fee_f.checked_mul(&difference).ok_or(CurveAmmError::Math)?;
                // new_pool_balance = new_balance - (fees[i] * admin_fee)
                let new_pool_balance =
                    (|| new_balance.checked_sub(&fees[i].checked_mul(&admin_fee_f)?))()
                        .ok_or(CurveAmmError::Math)?;

                let pool_mut = self
                    .pools
                    .get_mut(&pool_id)
                    .ok_or(CurveAmmError::PoolNotFound)?;
                pool_mut.balances[i] = new_pool_balance;

                new_balances[i] = new_balances[i]
                    .checked_sub(&fees[i])
                    .ok_or(CurveAmmError::Math)?;
            }
            let d2 = self.get_d(&new_balances, ann).ok_or(CurveAmmError::Math)?;

            // mint_amount = token_supply * (d2 - d0) / d0
            mint_amount = (|| {
                token_supply
                    .checked_mul(&d2.checked_sub(&d0)?)?
                    .checked_div(&d0)
            })()
            .ok_or(CurveAmmError::Math)?;
        } else {
            let pool_mut = self
                .pools
                .get_mut(&pool_id)
                .ok_or(CurveAmmError::PoolNotFound)?;
            pool_mut.balances = new_balances;
            mint_amount = d1;
        }
        let pool = self
            .pools
            .get(&pool_id)
            .ok_or(CurveAmmError::PoolNotFound)?;
        ensure!(
            mint_amount >= min_mint_amount,
            CurveAmmError::RequiredAmountNotReached
        );

        let _new_token_supply = token_supply
            .checked_add(&mint_amount)
            .ok_or(CurveAmmError::Math)?;

        // Ensure that for all tokens user has sufficient amount
        for (i, amount) in amounts.iter().enumerate() {
            let balance;
            let reply: Event = msg::send_and_wait_for_reply(
                pool.assets[i],
                &Action::BalanceOf(H256::from_slice(who.as_ref())),
                1_000_000_000,
                0,
            )
            .await
            .expect("Error in async message");
            // if let Ok(Event::Balance(bal)) = Event::decode(&mut reply) {
            //     balance = bal;
            // } else {
            //     panic!("could not decode TotalIssuance reply");
            // }
            match reply {
                Event::Balance(bal) => {
                    balance = bal;
                }
                _ => {
                    panic!("could not decode BalanceOf message");
                }
            }
            let balance: FixedU128 = FixedU128::from_inner(balance);
            ensure!(balance >= *amount, CurveAmmError::InsufficientFunds);
        }
        // Transfer funds to pool
        // TODO: fix to address
        for (i, amount) in amounts.iter().enumerate() {
            if amount > &zero {
                let _reply: Event = msg::send_and_wait_for_reply(
                    pool.assets[i],
                    &Action::Transfer(TransferData {
                        from: H256::from_slice(who.as_ref()),
                        to: H256::zero(),
                        amount: amount.into_inner(),
                    }),
                    1_000_000_000,
                    0,
                )
                .await
                .expect("could not decode Transfer reply");
            }
        }
        //TODO : check if following is correct or not.
        let mint_amount: u128 = mint_amount.into_inner() / FixedU128::DIV;

        let _reply: Event = msg::send_and_wait_for_reply(
            pool.pool_asset,
            &Action::Mint(MintInput {
                account: H256::from_slice(who.as_ref()),
                amount: mint_amount,
            }),
            1_000_000_000,
            0,
        )
        .await
        .expect("could not decode mint reply");

        //TODO: fees related stuff.

        // TODO: send msg in reply
        // Self::deposit_event(Event::LiquidityAdded {
        //     who: provider,
        //     pool_id,
        //     token_amounts,
        //     fees,
        //     invariant,
        //     token_supply,
        //     mint_amount,
        // });

        Ok(())
    }
    #[allow(dead_code)]
    async fn remove_liquidity(
        &mut self,
        who: &ActorId,
        pool_id: PoolId,
        amount: FixedU128,
    ) -> Result<(), CurveAmmError> {
        let zero = FixedU128::zero();
        ensure!(amount >= zero, CurveAmmError::WrongAssetAmount);
        let pool = self
            .pools
            .get_mut(&pool_id)
            .ok_or(CurveAmmError::PoolNotFound)?;
        let n_coins = pool.assets.len();
        ensure!(
            n_coins == pool.balances.len(),
            CurveAmmError::InconsistentStorage
        );
        let token_supply;
        let reply: Event =
            msg::send_and_wait_for_reply(pool.pool_asset, &Action::TotalIssuance, 1_000_000_000, 0)
                .await
                .expect("Error in async message");
        match reply {
            Event::TotalIssuance(bal) => {
                token_supply = FixedU128::saturating_from_integer(bal);
            }
            _ => {
                panic!("could not decode TotalIssuance reply");
            }
        }
        let mut n_amounts = vec![FixedU128::zero(); n_coins];
        for (i, n_amount) in n_amounts.iter_mut().enumerate().take(n_coins) {
            // for i in 0..n_coins {
            let old_balance = pool.balances[i];
            // value = old_balance * n_amount / token_supply
            let value = (|| {
                old_balance
                    .checked_mul(n_amount)?
                    .checked_div(&token_supply)
            })()
            .ok_or(CurveAmmError::Math)?;
            // pool.balances[i] = old_balance - value
            pool.balances[i] = old_balance
                .checked_sub(&value)
                .ok_or(CurveAmmError::InsufficientFunds)?;

            *n_amount = value;
        }
        //TODO : check if following is correct or not.
        let burn_amount: u128 = amount.into_inner() / FixedU128::DIV;
        let _reply: Event = msg::send_and_wait_for_reply(
            pool.pool_asset,
            &Action::Burn(BurnInput {
                account: H256::from_slice(who.as_ref()),
                amount: burn_amount,
            }),
            1_000_000_000,
            0,
        )
        .await
        .expect("could not decode burn reply");
        // for i in 0..n_coins {
        for (i, n_amount) in n_amounts.iter_mut().enumerate().take(n_coins) {
            let balance;
            let reply: Event = msg::send_and_wait_for_reply(
                pool.assets[i],
                //TODO: change this to smart contract's self address
                &Action::BalanceOf(H256::from_slice(who.as_ref())),
                1_000_000_000,
                0,
            )
            .await
            .expect("Error in async message");
            match reply {
                Event::Balance(bal) => {
                    balance = bal;
                }
                _ => {
                    panic!("could not decode BalanceOf message");
                }
            }
            // TODO: check if following is correct or not
            let balance: FixedU128 = FixedU128::from_inner(balance / FixedU128::DIV);
            ensure!(balance >= *n_amount, CurveAmmError::InsufficientFunds);
        }
        // Transfer funds from pool
        // TODO: fix to address
        for (i, amount) in n_amounts.iter().enumerate() {
            if amount > &zero {
                let amount: u128 = amount.into_inner() / FixedU128::DIV;
                let _reply: Event = msg::send_and_wait_for_reply(
                    pool.assets[i],
                    &Action::Transfer(TransferData {
                        from: H256::zero(),
                        to: H256::from_slice(who.as_ref()),
                        amount,
                    }),
                    1_000_000_000,
                    0,
                )
                .await
                .expect("could not decode Transfer reply");
            }
        }

        Ok(())
    }

    #[allow(dead_code)]
    async fn exchange(
        &mut self,
        who: &ActorId,
        pool_id: PoolId,
        i: usize,
        j: usize,
        dx: FixedU128,
    ) -> Result<(), CurveAmmError> {
        let prec = self.get_precision();
        let zero = FixedU128::zero();
        ensure!(dx >= zero, CurveAmmError::WrongAssetAmount);
        let pool = self
            .pools
            .get(&pool_id)
            .ok_or(CurveAmmError::PoolNotFound)?;
        let amp_coeff = pool.amplification_coefficient;
        let n_coins = pool.assets.len();
        ensure!(
            n_coins == pool.balances.len(),
            CurveAmmError::InconsistentStorage
        );
        ensure!(i < n_coins && j < n_coins, CurveAmmError::IndexOutOfRange);
        let xp = pool.balances.clone();
        let x = xp[i].checked_add(&dx).ok_or(CurveAmmError::Math)?;
        let ann = self
            .get_ann(amp_coeff, n_coins)
            .ok_or(CurveAmmError::Math)?;
        let y = self.get_y(i, j, x, &xp, ann).ok_or(CurveAmmError::Math)?;
        let dy = (|| xp[j].checked_sub(&y)?.checked_sub(&prec))().ok_or(CurveAmmError::Math)?;

        let pool = self
            .pools
            .get_mut(&pool_id)
            .ok_or(CurveAmmError::PoolNotFound)?;
        pool.balances[i] = xp[i].checked_add(&dx).ok_or(CurveAmmError::Math)?;
        pool.balances[j] = xp[j].checked_sub(&dy).ok_or(CurveAmmError::Math)?;
        let pool = self
            .pools
            .get(&pool_id)
            .ok_or(CurveAmmError::PoolNotFound)?;
        let balance;
        let reply: Event = msg::send_and_wait_for_reply(
            pool.assets[i],
            &Action::BalanceOf(H256::from_slice(who.as_ref())),
            1_000_000_000,
            0,
        )
        .await
        .expect("Error in async message");
        match reply {
            Event::Balance(bal) => {
                balance = bal;
            }
            _ => {
                panic!("could not decode BalanceOf message");
            }
        }
        // TODO: check if following is correct or not
        let balance: FixedU128 = FixedU128::from_inner(balance / FixedU128::DIV);
        ensure!(balance >= dx, CurveAmmError::InsufficientFunds);
        let balance;
        let reply: Event = msg::send_and_wait_for_reply(
            pool.assets[j],
            // TODO: Fix below to be smart contract's address
            &Action::BalanceOf(H256::zero()),
            1_000_000_000,
            0,
        )
        .await
        .expect("Error in async message");
        match reply {
            Event::Balance(bal) => {
                balance = bal;
            }
            _ => {
                panic!("could not decode BalanceOf message");
            }
        }
        // TODO: check if following is correct or not
        let balance: FixedU128 = FixedU128::from_inner(balance / FixedU128::DIV);
        let amount: u128 = dx.into_inner() / FixedU128::DIV;
        ensure!(balance >= dy, CurveAmmError::InsufficientFunds);
        let _reply: Event = msg::send_and_wait_for_reply(
            pool.assets[i],
            &Action::Transfer(TransferData {
                from: H256::from_slice(who.as_ref()),
                to: H256::zero(),
                amount,
            }),
            1_000_000_000,
            0,
        )
        .await
        .expect("could not decode Transfer reply");
        let amount: u128 = dy.into_inner() / FixedU128::DIV;
        let _reply: Event = msg::send_and_wait_for_reply(
            pool.assets[j],
            &Action::Transfer(TransferData {
                from: H256::zero(),
                to: H256::from_slice(who.as_ref()),
                amount,
            }),
            1_000_000_000,
            0,
        )
        .await
        .expect("could not decode Transfer reply");
        Ok(())
    }
}

static mut CURVE_AMM: CurveAmm = CurveAmm {
    pool_count: 0,
    pools: BTreeMap::new(),
};

// #[no_mangle]
// pub unsafe extern "C" fn handle() {
// let action: Action = msg::load().expect("Could not load Action");

// match action {
// }
// }

#[no_mangle]
pub unsafe extern "C" fn init() {
    let config: CurveAmmInitConfig = msg::load().expect("Unable to decode InitConfig");
    debug!("CurveAmm InitConfig {:?}", config);
    let owner = ActorId::new(config.owner.to_fixed_bytes());
    let x_token = ActorId::new(config.x_token_program_id.to_fixed_bytes());
    let y_token = ActorId::new(config.y_token_program_id.to_fixed_bytes());
    let lp_token = ActorId::new(config.lp_token_program_id.to_fixed_bytes());
    let assets = vec![x_token, y_token];
    let amplification_coefficient =
        FixedU128::saturating_from_integer(config.amplification_coefficient);
    let fee = Permill::from_percent(config.fee);
    let admin_fee = Permill::from_percent(config.admin_fee);
    debug!("owner {:?} x_token {:?} y_token {:?} lp_token {:?} amplification_coefficient {:?} fee {:?} admin_fee {:?}", owner, x_token, y_token, lp_token, amplification_coefficient, fee, admin_fee);
    let res = CURVE_AMM
        .create_pool(
            &owner,
            assets,
            &lp_token,
            amplification_coefficient,
            fee,
            admin_fee,
        )
        .expect("Pool creation failed");
    msg::reply(res, exec::gas_available() - GAS_RESERVE, 0);
}

#[gstd::async_main]
async fn main() {}
