use crate::{mock::*, data::*};
use frame_support::assert_ok;

#[test]
fn it_works_for_default_value() {
	new_test_ext().execute_with(|| {
		// Dispatch a signed extrinsic.
		assert_ok!(
			GearModule::submit_program(
				Origin::signed(1),
				Program { static_pages: Vec::new(), code: Vec::new() }
			)
		);
	});
}
