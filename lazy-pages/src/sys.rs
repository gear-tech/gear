use crate::{LazyPage, LazyPageError, WASM_MEM_BEGIN};
use cfg_if::cfg_if;
use gear_core::memory::{PageBuf, PageNumber};
use region::Protection;

cfg_if! {
    if #[cfg(windows)] {
        mod windows;
        pub use windows::*;
    } else if #[cfg(unix)] {
        mod unix;
        pub use unix::*;
    } else {
        compile_error!("lazy pages are not supported on your system. Disable `lazy-pages` feature");
    }
}

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
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
    #[display(
        fmt = "Page data must contain {} bytes, actually has {}",
        expected,
        actual
    )]
    InvalidPageSize { expected: usize, actual: u32 },
    #[display(fmt = "Protection error: {}", _0)]
    #[from]
    Protect(region::Error),
    #[display(fmt = "Lazy page error: {}", _0)]
    #[from]
    LazyPage(LazyPageError),
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
pub unsafe fn user_signal_handler(info: ExceptionInfo) -> Result<(), Error> {
    let native_ps = region::page::size();
    let gear_ps = PageNumber::size();

    log::debug!("Interrupted, exception info = {:?}", info);

    let mem = info.fault_addr;
    let native_page = region::page::floor(mem) as usize;
    let wasm_mem_begin = WASM_MEM_BEGIN.with(|x| *x.borrow()) as usize;

    if wasm_mem_begin == 0 {
        return Err(Error::WasmBeginIsNotSet);
    }

    if wasm_mem_begin > native_page {
        return Err(Error::UnknownMemory {
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
        let page = LazyPage::from(gear_page) + idx;

        let hash_key_in_storage = page.take_info()?;
        let ptr = (unprot_addr as *mut u8).add(idx as usize * gear_ps);
        let buffer_as_slice = std::slice::from_raw_parts_mut(ptr, gear_ps);

        let res = sp_io::storage::read(&hash_key_in_storage, buffer_as_slice, 0);

        if res.is_none() {
            log::trace!(
                "Page #{} has no data in storage, so just save current page data to released pages",
                page
            );
        } else {
            log::trace!("Page #{} has data in storage, so set this data for page and save it in released pages", page);
        }

        if let Some(size) = res.filter(|&size| size as usize != PageNumber::size()) {
            return Err(Error::InvalidPageSize {
                expected: PageNumber::size(),
                actual: size,
            });
        }

        let page_buf = PageBuf::new_from_vec(buffer_as_slice.to_vec())
            .expect("Cannot panic here, because we create slice with PageBuf size");
        page.release(page_buf)?;
    }

    Ok(())
}
