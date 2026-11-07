//! Real-Windows test: DPAPI wrap/unwrap, and tamper detection.
#![cfg(windows)]

use washi::platform::KeyProtector;
use washi::win::DpapiProtector;

#[test]
fn wrap_and_unwrap_round_trips() {
    let p = DpapiProtector;
    let key = p.generate_key().unwrap();
    assert_ne!(*key, [0u8; 32], "generated key must not be all zero");
    assert_ne!(*key, *p.generate_key().unwrap(), "keys must be random");
    let wrapped = p.protect(&*key).unwrap();
    assert!(!wrapped.windows(32).any(|w| w == &*key), "wrapped blob must not contain the raw key");
    assert_eq!(&p.unprotect(&wrapped).unwrap()[..], &key[..]);
}

#[test]
fn a_one_byte_change_fails_cleanly() {
    let p = DpapiProtector;
    let wrapped = p.protect(&*p.generate_key().unwrap()).unwrap();
    for i in [0, wrapped.len() / 2, wrapped.len() - 1] {
        let mut bad = wrapped.clone();
        bad[i] ^= 1;
        assert!(p.unprotect(&bad).is_err(), "flipping byte {i} must be detected");
    }
    assert!(p.unprotect(&wrapped[..wrapped.len() - 1]).is_err(), "truncation must be detected");
    assert!(p.unprotect(&[]).is_err());
}
