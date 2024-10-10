use jsonrpsee::types::ErrorObject;

pub fn db_err(err: &'static str) -> ErrorObject<'static> {
    ErrorObject::owned(8000, "Database error", Some(err))
}

pub fn runtime_err(err: anyhow::Error) -> ErrorObject<'static> {
    ErrorObject::owned(8000, "Runtime error", Some(format!("{err}")))
}

pub fn internal() -> ErrorObject<'static> {
    ErrorObject::owned(8000, "Internal error", None::<&str>)
}
