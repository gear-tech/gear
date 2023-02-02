// TODO
//
// rename or remove this module when supporting both gear
// and vara in one build.
#[allow(clippy::all, missing_docs)]
pub mod api {
    include!(concat!(env!("OUT_DIR"), "/metadata.rs"));

    pub use metadata::*;

    #[cfg(any(
        all(feature = "gear", not(feature = "vara")),
        all(feature = "gear", feature = "vara")
    ))]
    pub use metadata::runtime_types::gear_runtime::RuntimeEvent;

    #[cfg(all(feature = "vara", not(feature = "gear")))]
    pub use metadata::runtime_types::vara_runtime::RuntimeEvent;
}

mod impls;
