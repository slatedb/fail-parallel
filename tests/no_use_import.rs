// Copyright 2022 TiKV Project Authors. Licensed under Apache-2.0.

use std::*;

use fail_parallel::{fail_point, FailPointRegistry};
use sync::Arc;

#[test]
#[cfg_attr(not(feature = "failpoints"), ignore)]
fn test_return() {
    let registry = Arc::new(FailPointRegistry::new());
    let f = || {
        fail_point!(registry.clone(), "return", |s: Option<String>| s
            .map_or(2, |s| s.parse().unwrap()));
        0
    };
    assert_eq!(f(), 0);

    fail_parallel::cfg(registry.clone(), "return", "return(1000)").unwrap();
    assert_eq!(f(), 1000);

    fail_parallel::cfg(registry.clone(), "return", "return").unwrap();
    assert_eq!(f(), 2);
}
