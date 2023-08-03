// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

use crate::{pallet, ExtManager};
use alloc::collections::{BTreeMap, BTreeSet};
use common::Origin;
use core::marker::PhantomData;
use gear_core::{ids::ProgramId, memory::PageBuf, pages::GearPage};

/// Manager that handles memory pages of a program.
pub(crate) struct PagesManager<T> {
    details: DefaultPagesManagerDetails<T>,
}

impl<T> PagesManager<T>
where
    T: pallet::Config,
    T::AccountId: Origin,
{
    /// Enable pages management.
    ///
    /// Allowed to be called many times.
    pub(crate) fn enable() -> Self {
        Self {
            details: DefaultPagesManagerDetails::enable(),
        }
    }

    /// Get memory pages of program.
    pub fn memory_pages(
        &self,
        program_id: ProgramId,
        pages_with_data: &BTreeSet<GearPage>,
    ) -> Option<BTreeMap<GearPage, PageBuf>> {
        self.details.memory_pages(program_id, pages_with_data)
    }

    /// Get memory pages and track program in [`ExtManager`].
    pub(crate) fn get_and_track_memory_pages(
        &self,
        manager: &mut ExtManager<T>,
        program_id: ProgramId,
        pages_with_data: &BTreeSet<GearPage>,
    ) -> Option<BTreeMap<GearPage, PageBuf>> {
        let pages = self.memory_pages(program_id, pages_with_data);
        manager.insert_program_id_loaded_pages(program_id);
        pages
    }
}

#[cfg(not(feature = "lazy-pages"))]
pub(crate) type DefaultPagesManagerDetails<T> = NoopPagesManager<T>;

#[cfg(feature = "lazy-pages")]
pub(crate) type DefaultPagesManagerDetails<T> = lazy_pages::LazyPagesManager<T>;

/// Pages manager implementation details.
trait PagesManagerDetails<T>
where
    T: pallet::Config,
{
    fn enable() -> Self;

    fn memory_pages(
        &self,
        program_id: ProgramId,
        pages_with_data: &BTreeSet<GearPage>,
    ) -> Option<BTreeMap<GearPage, PageBuf>>;
}

/// Manager that literally does nothing.
pub(crate) struct NoopPagesManager<T>(PhantomData<T>);

impl<T> PagesManagerDetails<T> for NoopPagesManager<T>
where
    T: pallet::Config,
{
    fn enable() -> Self {
        Self(PhantomData)
    }

    fn memory_pages(
        &self,
        _program_id: ProgramId,
        _pages_with_data: &BTreeSet<GearPage>,
    ) -> Option<BTreeMap<GearPage, PageBuf>> {
        Some(Default::default())
    }
}

#[cfg(feature = "lazy-pages")]
mod lazy_pages {
    use crate::{pages_manager::PagesManagerDetails, Config, ProgramStorageOf};
    use common::ProgramStorage;
    use core::marker::PhantomData;
    use gear_core::{ids::ProgramId, memory::PageBuf, pages::GearPage};
    use std::collections::{BTreeMap, BTreeSet};

    /// Manager that works with [`gear_lazy_pages_common`].
    pub(crate) struct LazyPagesManager<T>(PhantomData<T>);

    impl<T> PagesManagerDetails<T> for LazyPagesManager<T>
    where
        T: Config,
    {
        fn enable() -> Self {
            let prefix = ProgramStorageOf::<T>::pages_final_prefix();
            if !gear_lazy_pages_common::try_to_enable_lazy_pages(prefix) {
                unreachable!("By some reasons we cannot run lazy-pages on this machine");
            }

            Self(PhantomData)
        }

        fn memory_pages(
            &self,
            program_id: ProgramId,
            pages_with_data: &BTreeSet<GearPage>,
        ) -> Option<BTreeMap<GearPage, PageBuf>> {
            match ProgramStorageOf::<T>::get_program_data_for_pages(
                program_id,
                pages_with_data.iter(),
            ) {
                Ok(data) => Some(data),
                Err(err) => {
                    log::error!("Cannot get data for program pages: {err:?}");
                    None
                }
            }
        }
    }
}
