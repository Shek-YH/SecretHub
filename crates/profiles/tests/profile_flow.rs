use secrethub_profiles::{apply_profile, Profile};

#[test]
fn applies_profile_in_saved_order_and_reports_missing_ids() {
    let profile = Profile::new("AI Standard", vec!["openai".into(), "gemini".into(), "missing".into()]);
    let result = apply_profile(&profile, &["gemini".into(), "openai".into()]);

    assert_eq!(result.available_ids, vec!["openai", "gemini"]);
    assert_eq!(result.missing_ids, vec!["missing"]);
}
