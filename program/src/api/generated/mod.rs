// TODO
//
// rename or remove this module when supporting both gear
// and vara in one build.
#[allow(clippy::all, missing_docs)]
pub mod api {
    include!(concat!(env!("OUT_DIR"), "/metadata.rs"));

    pub use metadata::{runtime_types::gear_runtime::RuntimeEvent, *};
}

mod impls;
