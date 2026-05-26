use sfo_reuseport::Error;

#[test]
fn handler_error_is_distinguishable() {
    let error = Error::Handler("boom".to_string());
    assert!(error.to_string().contains("handler error"));
}
