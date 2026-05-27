use crate::HostState;
use wasmtime::{Memory, Table};

#[derive(Default)]
pub struct StoreData {
	/// This will only be set when we call into the runtime.
	pub host_state: Option<HostState>,
	/// This will be always set once the store is initialized.
	pub memory: Option<Memory>,
	/// This will be set only if the runtime actually contains a table.
	pub table: Option<Table>,
}

impl StoreData {
	/// Returns a mutable reference to the host state.
	pub fn host_state_mut(&mut self) -> Option<&mut HostState> {
		self.host_state.as_mut()
	}

	/// Returns the host memory.
	pub fn memory(&self) -> Memory {
		self.memory.expect("memory is always set; qed")
	}
}
