// TODO
//
// rename or remove this module when supporting both gear
// and vara in one build.
#[allow(clippy::all, missing_docs)]
pub mod api {
    include!(concat!(env!("OUT_DIR"), "/metadata.rs"));

    #[cfg(any(
        all(feature = "gear", feature = "vara"),
        all(feature = "gear", not(feature = "vara"))
    ))]
    pub use gear_metadata::*;

    #[cfg(any(
        all(feature = "gear", feature = "vara"),
        all(feature = "gear", not(feature = "vara"))
    ))]
    pub use gear_metadata::runtime_types::gear_runtime::RuntimeEvent;

    #[cfg(all(feature = "vara", not(feature = "gear")))]
    pub use vara_metadata::*;

    #[cfg(all(feature = "vara", not(feature = "gear")))]
    pub use vara_metadata::runtime_types::vara_runtime::RuntimeEvent;
}

mod impls;
