// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::db::OnChainStorageRO;
use ethexe_db::Database;
use std::fmt;

pub(crate) mod discovery;
pub(crate) mod list;
pub(crate) mod topic;

#[auto_impl::auto_impl(&, Box)]
pub trait ValidatorDatabase: Send + fmt::Debug + OnChainStorageRO {
    fn clone_boxed(&self) -> Box<dyn ValidatorDatabase>;
}

impl ValidatorDatabase for Database {
    fn clone_boxed(&self) -> Box<dyn ValidatorDatabase> {
        Box::new(self.clone())
    }
}
