use crate::{LAZY_PAGES_ENABLED, LAZY_PAGES_INFO, RELEASED_LAZY_PAGES, WASM_MEM_BEGIN};
use cfg_if::cfg_if;
use gear_core::memory::{PageBuf, PageNumber, WasmPageNumber};
use region::Protection;

cfg_if! {
    if #[cfg(windows)] {
        mod windows;
        pub use windows::*;
    } else if #[cfg(unix)] {
        mod unix;
        pub use unix::*;
    } else {
        compile_error!("lazy pages are not supported on your system. Disable `lazy-pages` feature")
    }
}

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum ExceptionHandlerError {
    #[display(fmt = "WASM memory begin address is not set")]
    WasmBeginIsNotSet,
    #[display(
        fmt = "Exception is from unknown memory (WASM {:#x} > native page {:x})",
        wasm_mem_begin,
        native_page
    )]
    UnknownMemory {
        wasm_mem_begin: usize,
        native_page: usize,
    },
    #[display(fmt = "Exception is from unknown memory: {:#x}", page)]
    LazyPagesUnknownInfo { page: u32 },
    #[display(
        fmt = "Page data must contain {} bytes, actually has {}",
        expected,
        actual
    )]
    InvalidPageSize { expected: usize, actual: u32 },
    #[display(fmt = "Page #{} cannot be released twice", _0)]
    PageDoubleRelease(u32),
    #[display(fmt = "Protection error: {}", _0)]
    #[from]
    Protect(region::Error),
}

#[derive(Debug)]
pub struct ExceptionInfo {
    /// Address where fault is occurred
    pub fault_addr: *const (),
}

/// Before contract execution some pages from wasm memory buffer are protected,
/// and cannot be accessed anyhow. When wasm executer tries to access one of these pages,
/// OS emits sigsegv or sigbus or EXCEPTION_ACCESS_VIOLATION. We handle the signal in this function.
/// Using OS signal info, we identify memory location and wasm page.
/// We remove read and write protections for page,
/// then we load wasm page data from storage to wasm page memory location.
/// Also we save page data to [RELEASED_LAZY_PAGES] in order to identify later
/// whether page is changed after execution.
/// After signal handler is done, OS returns execution to the same machine
/// instruction, which cause signal. Now memory which this instruction accesses
/// is not protected and with correct data.
pub unsafe fn memory_exception_handler(info: ExceptionInfo) -> Result<(), ExceptionHandlerError> {
    let native_ps = region::page::size();
    let gear_ps = PageNumber::size();

    log::debug!("Interrupted, exception info = {:?}", info);

    let mem = info.fault_addr;
    let native_page = region::page::floor(mem) as usize;
    let wasm_mem_begin = WASM_MEM_BEGIN.with(|x| *x.borrow()) as usize;

    if wasm_mem_begin == 0 {
        return Err(ExceptionHandlerError::WasmBeginIsNotSet);
    }

    if wasm_mem_begin > native_page {
        return Err(ExceptionHandlerError::UnknownMemory {
            wasm_mem_begin,
            native_page,
        });
    }

    // First gear page which must be unprotected
    let gear_page = PageNumber::from(((native_page - wasm_mem_begin) / gear_ps) as u32);

    let (gear_page, gear_pages_num, unprot_addr) = if native_ps > gear_ps {
        assert_eq!(native_ps % gear_ps, 0);
        (gear_page, native_ps / gear_ps, native_page)
    } else {
        assert_eq!(gear_ps % native_ps, 0);
        (gear_page, 1usize, wasm_mem_begin + gear_page.offset())
    };

    let accessed_page = PageNumber::from(((mem as usize - wasm_mem_begin) / gear_ps) as u32);
    log::debug!(
        "mem={:?} accessed={:?},{:?} pages={:?} page_native_addr={:#x}",
        mem,
        accessed_page,
        accessed_page.to_wasm_page(),
        gear_page.0..gear_page.0 + gear_pages_num as u32,
        unprot_addr
    );

    let unprot_size = gear_pages_num * gear_ps;

    region::protect(unprot_addr as *mut (), unprot_size, Protection::READ_WRITE)?;

    for idx in 0..gear_pages_num as u32 {
        let page = gear_page.0 + idx;

        let hash_key_in_storage = LAZY_PAGES_INFO
            .with(|info| info.borrow_mut().remove(&page))
            .ok_or(ExceptionHandlerError::LazyPagesUnknownInfo { page })?;

        let ptr = (unprot_addr as *mut u8).add(idx as usize * gear_ps);
        let buffer_as_slice = std::slice::from_raw_parts_mut(ptr, gear_ps);

        let res = sp_io::storage::read(&hash_key_in_storage, buffer_as_slice, 0);

        if res.is_none() {
            log::trace!("Page {:?} has no data in storage, so just save current page data to released pages", page);
        } else {
            log::trace!("Page {:?} has data in storage, so set this data for page and save it in released pages", page);
        }

        if let Some(size) = res.filter(|&size| size as usize != PageNumber::size()) {
            return Err(ExceptionHandlerError::InvalidPageSize {
                expected: PageNumber::size(),
                actual: size,
            });
        }

        RELEASED_LAZY_PAGES.with(|released_pages| {
            let page_buf = PageBuf::new_from_vec(buffer_as_slice.to_vec())
                .expect("Cannot panic here, because we create slice with PageBuf size");
            // Restrict any page handling in signal handler more then one time.
            // If some page will be released twice it means, that this page has been added
            // to lazy pages more then one time during current execution.
            // This situation may cause problems with memory data update in storage.
            // For example: one page has no data in storage, but allocated for current program.
            // Let's make some action for it:
            // 1) Change data in page: Default data  ->  Data1
            // 2) Free page
            // 3) Alloc page, data will Data2 (may be equal Data1).
            // 4) After alloc we can set page as lazy, to identify wether page is changed after allocation.
            // This means that we can skip page update in storage in case it wasnt changed after allocation.
            // 5) Write some data in page but do not change it Data2 -> Data2.
            // During this step signal handler writes Data2 as data for released page.
            // 6) After execution we will have Data2 in page. And Data2 in released. So, nothing will be updated
            // in storage. But program may have some significant data for next execution - so we have a bug.
            // To avoid this we restrict double releasing.
            // You can also check another cases in test: memory_access_cases.
            let res = released_pages.borrow_mut().insert(page, Some(page_buf));
            if res.is_some() {
                Err(ExceptionHandlerError::PageDoubleRelease(page))
            } else {
                Ok(())
            }
        })?;
    }

    Ok(())
}

/// Initialize lazy pages:
/// 1) checks whether lazy pages is supported in current environment
/// 2) set signals handler
pub unsafe fn init() -> bool {
    if LAZY_PAGES_ENABLED.with(|x| *x.borrow()) {
        log::trace!("Lazy-pages has been already enabled");
        return true;
    }

    if !LAZY_PAGES_INFO.with(|x| x.borrow().is_empty()) {
        log::error!("Lazy pages info must be empty before initialization");
        return false;
    }

    if !WASM_MEM_BEGIN.with(|x| *x.borrow() == 0) {
        log::error!("Wasm mem begin must be 0 before initialization");
        return false;
    }

    if !RELEASED_LAZY_PAGES.with(|x| x.borrow().is_empty()) {
        log::error!("Released lazy pages must be empty before initialization");
        return false;
    }

    let ps = region::page::size();
    if ps > WasmPageNumber::size()
        || WasmPageNumber::size() % ps != 0
        || (ps > PageNumber::size() && ps % PageNumber::size() != 0)
        || (ps < PageNumber::size() && PageNumber::size() % ps != 0)
    {
        log::debug!("Unsupported native pages size: {:#x}", ps);
        return false;
    }

    if let Err(err) = setup_memory_exception_handler() {
        log::debug!("Failed to setup memory exception handler: {}", err);
        return false;
    }

    log::debug!("Lazy pages are successfully enabled");
    LAZY_PAGES_ENABLED.with(|x| *x.borrow_mut() = true);

    true
}
