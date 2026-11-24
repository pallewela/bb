//! `Repo::parse`: the `-R` flag, the `origin` remote URL and PR URLs all go through it.
#![no_main]
use bitbucket_bb_cli::context::Repo;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else { return };
    let Ok(r) = Repo::parse(s) else { return };
    // A parsed repo names exactly one workspace and one repository. `api()` and `web()` paste
    // them into URLs unescaped, so a component must not be empty or carry a separator, a URL
    // query/fragment, whitespace or control characters: the request would go somewhere else.
    for part in [&r.workspace, &r.slug] {
        assert!(!part.is_empty(), "empty component: {s:?} -> {r:?}");
        assert!(
            !part.chars().any(|c| matches!(c, '/' | '?' | '#') || c.is_whitespace() || c.is_control()),
            "bad character in component: {s:?} -> {r:?}"
        );
    }
    // The WORKSPACE/REPO form of a parsed repo parses back to the same repo.
    let again = Repo::parse(&format!("{}/{}", r.workspace, r.slug)).ok();
    assert_eq!(again.as_ref(), Some(&r), "does not round-trip: {s:?} -> {r:?}");
});
