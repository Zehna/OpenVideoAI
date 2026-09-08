use openvideoai_lib::logging::init_logging;

#[test]
fn test_logging_initialization() {
    // Calling init_logging should succeed or handle multiple initializations gracefully
    let res = init_logging();
    assert!(
        res.is_ok(),
        "Logging initialization failed: {:?}",
        res.err()
    );
}
