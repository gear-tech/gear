use gstd::*;

struct SomeType(usize);

#[derive(Debug)]
struct SomeError;

#[test]
fn bail_ok() {
    let res: Result<SomeType, SomeError> = Ok(SomeType(0));
    let val = bail!(res, "Your static explanation for both features");
    assert_eq!(val.0, 0);

    let res: Result<SomeType, SomeError> = Ok(SomeType(1));
    let val = bail!(
        res,
        "Your static release explanation",
        "Your static debug explanation"
    );
    assert_eq!(val.0, 1);

    let res: Result<SomeType, SomeError> = Ok(SomeType(2));
    let val = bail!(
        res,
        "Your static release explanation",
        "It was formatted -> {}",
        0
    );
    assert_eq!(val.0, 2);

    let res: Result<SomeType, SomeError> = Ok(SomeType(3));
    let val = bail!(
        res,
        "Your static release explanation",
        "They were formatted -> {} {}",
        0,
        "SECOND_ARG"
    );
    assert_eq!(val.0, 3);
}

#[test]
#[should_panic(expected = "Your static explanation for both features")]
fn bail_err_general_message() {
    let res: Result<SomeType, SomeError> = Err(SomeError);

    bail!(res, "Your static explanation for both features");
}

#[test]
#[cfg(not(feature = "debug"))]
#[should_panic(expected = "Your static release explanation")]
fn bail_err_no_format() {
    let res: Result<SomeType, SomeError> = Err(SomeError);

    bail!(
        res,
        "Your static release explanation",
        "Your static debug explanation"
    );
}

#[test]
#[cfg(feature = "debug")]
#[should_panic(expected = "Your static debug explanation")]
fn bail_err_no_format() {
    let res: Result<SomeType, SomeError> = Err(SomeError);

    bail!(
        res,
        "Your static release explanation",
        "Your static debug explanation"
    );
}

#[test]
#[cfg(not(feature = "debug"))]
#[should_panic(expected = "Your static release explanation")]
fn bail_err_single_arg_format() {
    let res: Result<SomeType, SomeError> = Err(SomeError);

    bail!(
        res,
        "Your static release explanation",
        "It was formatted -> {}",
        0
    );
}

#[test]
#[cfg(feature = "debug")]
#[should_panic(expected = "It was formatted -> 0: SomeError")]
fn bail_err_single_arg_format() {
    let res: Result<SomeType, SomeError> = Err(SomeError);

    bail!(
        res,
        "Your static release explanation",
        "It was formatted -> {}",
        0
    );
}

#[test]
#[cfg(not(feature = "debug"))]
#[should_panic(expected = "Your static release explanation")]
fn bail_err_multiple_args_format() {
    let res: Result<SomeType, SomeError> = Err(SomeError);

    bail!(
        res,
        "Your static release explanation",
        "They were formatted -> {} {}",
        0,
        "SECOND_ARG"
    );
}

#[test]
#[cfg(feature = "debug")]
#[should_panic(expected = "They were formatted -> 0 SECOND_ARG: SomeError")]
fn bail_err_multiple_args_format() {
    let res: Result<SomeType, SomeError> = Err(SomeError);

    bail!(
        res,
        "Your static release explanation",
        "They were formatted -> {} {}",
        0,
        "SECOND_ARG"
    );
}
