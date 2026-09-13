use secrethub_proxy::{EndpointPolicy, TokenAuthority};

#[test]
fn token_is_short_lived_revocable_and_never_stores_plaintext_token() {
    let mut authority = TokenAuthority::new();
    let grant = authority.issue(100, 60);
    assert!(authority.authorize(&grant.token, 100));
    assert!(authority.authorize(&grant.token, 159));
    assert!(!authority.authorize(&grant.token, 160));
    assert!(!authority.authorize("not-the-token", 100));
    let second = authority.issue(100, 60);
    assert!(authority.revoke(&second.token));
    assert!(!authority.authorize(&second.token, 101));
    assert_eq!(authority.active_grants(), 0);
}

#[test]
fn endpoint_policy_allows_fixed_provider_origin_only() {
    let policy = EndpointPolicy::new("openai");
    assert!(policy
        .authorize("https://api.openai.com/v1/responses")
        .is_ok());
    assert!(policy
        .authorize("http://api.openai.com/v1/responses")
        .is_err());
    assert!(policy.authorize("https://127.0.0.1/v1/responses").is_err());
    assert!(EndpointPolicy::new("unknown")
        .authorize("https://example.invalid")
        .is_err());
}
