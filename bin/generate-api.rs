use gear_program::builder::Pre;

fn main() {
    let pre = Pre::default();

    if let Err((expected, _)) = pre.check_spec_version() {
        pre.build_gear();
        pre.generate_gear_api(expected).unwrap();
    }
}
