mod backend;
mod run;
mod runtime;

pub(crate) use backend::EthexeBackend;

pub(crate) fn init_lazy_pages() {
    runtime::init_lazy_pages();
}

#[cfg(test)]
mod tests;
