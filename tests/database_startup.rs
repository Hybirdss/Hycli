#[test]
fn concurrent_policy_startup_serializes_wal_initialization() {
    let mut failures = 0;
    for _ in 0..32 {
        let root = std::env::temp_dir().join(format!("hycli-policy-probe-{}", hycli::util::id()));
        let path = root.join("runtime.db");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    let result = hycli::policy::Policy::open(&path);
                    if let Err(error) = &result {
                        eprintln!("synthetic policy startup error: {}", error);
                    }
                    result.is_err()
                })
            })
            .collect();
        for worker in workers {
            failures += usize::from(worker.join().unwrap());
        }
    }
    assert_eq!(
        failures, 0,
        "concurrent policy startup must not race WAL initialization"
    );
}
