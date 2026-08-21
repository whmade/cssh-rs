//! Unit tests for [`crate::v1::capability`].

use super::*;

#[test]
fn test_check_version_accepts_equal_version() {
    assert!(check_version(ProtocolVersion::new(1, 0), ProtocolVersion::new(1, 0)).is_ok());
}

#[test]
fn test_check_version_accepts_differing_minor() {
    assert!(check_version(ProtocolVersion::new(1, 0), ProtocolVersion::new(1, 7)).is_ok());
    assert!(check_version(ProtocolVersion::new(1, 9), ProtocolVersion::new(1, 2)).is_ok());
}

#[test]
fn test_check_version_rejects_differing_major() {
    let err = check_version(ProtocolVersion::new(1, 4), ProtocolVersion::new(2, 0))
        .expect_err("major mismatch must be rejected");
    assert_eq!(err.local, ProtocolVersion::new(1, 4));
    assert_eq!(err.remote, ProtocolVersion::new(2, 0));
}

#[test]
fn test_negotiate_capabilities_returns_sorted_intersection() {
    let local = vec![
        "paste".to_string(),
        "activation_token".to_string(),
        "highlight".to_string(),
    ];
    let remote = vec![
        "highlight".to_string(),
        "paste".to_string(),
        "geometry".to_string(),
    ];
    assert_eq!(
        negotiate_capabilities(&local, &remote),
        vec!["highlight".to_string(), "paste".to_string()]
    );
}

#[test]
fn test_negotiate_capabilities_deduplicates() {
    let local = vec!["paste".to_string(), "paste".to_string()];
    let remote = vec!["paste".to_string()];
    assert_eq!(
        negotiate_capabilities(&local, &remote),
        vec!["paste".to_string()]
    );
}

#[test]
fn test_negotiate_capabilities_disjoint_is_empty() {
    let local = vec!["alpha".to_string()];
    let remote = vec!["beta".to_string()];
    assert!(negotiate_capabilities(&local, &remote).is_empty());
}

#[test]
fn test_negotiate_max_frame_len_takes_the_minimum() {
    assert_eq!(negotiate_max_frame_len(512, 4096), 512);
    assert_eq!(negotiate_max_frame_len(4096, 512), 512);
    assert_eq!(negotiate_max_frame_len(512, 512), 512);
}
