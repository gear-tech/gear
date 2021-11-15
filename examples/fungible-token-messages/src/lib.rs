#![no_std]

use codec::{Decode, Encode};
use gstd::prelude::*;
use primitive_types::H256;
use scale_info::TypeInfo;

#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct InitConfig {
    pub name: String,
    pub symbol: String,
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct MintInput {
    pub account: H256,
    pub amount: u128,
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct BurnInput {
    pub account: H256,
    pub amount: u128,
}

#[derive(Debug, Encode, Decode, TypeInfo)]
pub struct ApproveData {
    pub owner: H256,
    pub spender: H256,
    pub amount: u128,
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct TransferData {
    pub from: H256,
    pub to: H256,
    pub amount: u128,
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct TransferFromData {
    pub owner: H256,
    pub from: H256,
    pub to: H256,
    pub amount: u128,
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub enum Action {
    Mint(MintInput),
    Burn(BurnInput),
    Transfer(TransferData),
    TransferFrom(TransferFromData),
    Approve(ApproveData),
    IncreaseAllowance(ApproveData),
    DecreaseAllowance(ApproveData),
    TotalIssuance,
    BalanceOf(H256),
}

#[derive(Debug, Encode, Decode, TypeInfo)]
pub enum Event {
    Transfer(TransferData),
    Approval(ApproveData),
    TotalIssuance(u128),
    Balance(u128),
}
