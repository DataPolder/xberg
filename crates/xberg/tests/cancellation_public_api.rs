use xberg::ExtractionConfig;
use xberg::cancellation::CancellationToken;

#[test]
fn rust_callers_can_request_and_observe_cooperative_cancellation() {
    let caller_token = CancellationToken::default();
    let extraction_token = caller_token.clone();

    let config = ExtractionConfig {
        cancel_token: Some(extraction_token),
        ..Default::default()
    };

    assert!(
        !caller_token.is_cancelled(),
        "a newly created token must not contain a cancellation request"
    );

    caller_token.cancel();

    assert!(
        caller_token.is_cancelled(),
        "the caller's token must observe its cancellation request"
    );
    assert!(
        config
            .cancel_token
            .as_ref()
            .expect("the extraction config should retain its token")
            .is_cancelled(),
        "the token stored in ExtractionConfig must share the same cancellation state"
    );
}
