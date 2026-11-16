//! A web key is kept before its CSR goes out, so an order a restart cuts
//! short is finished with the same key: the CT watch never meets this
//! computer's own logged certificate as a stranger's.

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn an_unfinished_order_keeps_its_key_across_a_restart() {
    // The test relay orders no certificates, so every order fails after
    // its CSR is sent, as one cut short would.
    let (http, _) = private_relay().await;
    let dir = tempfile::tempdir().unwrap();
    let token_file = dir.path().join("relay.token");
    std::fs::write(&token_file, TOKEN).unwrap();
    let endpoint = Endpoint::parse(&format!("http://localhost:{}", http.port())).unwrap();
    let configured = vec![Relay {
        token_file: Some(token_file),
        web: true,
        ..endpoint.clone().into()
    }];
    let store = crate::relay::web::store_dir(dir.path(), &endpoint.to_string());
    let name = endpoint.to_string();
    let mut kept = Vec::new();
    for run in 0..2 {
        let mut shared = state(dir.path());
        let relays = Arc::new(Relays::load(dir.path(), configured.clone()).unwrap());
        Arc::get_mut(&mut shared).unwrap().relay = Some(Arc::clone(&relays));
        let (stop, stopped) = watch::channel(false);
        relays.start(served(dir.path(), &shared).0, stopped);
        let mut failed = false;
        for _ in 0..200 {
            failed = relays.status()[&name]
                .web
                .as_ref()
                .is_some_and(|web| web.error.is_some());
            if failed {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(failed, "run {run}: the order never failed");
        assert!(!store.join("chain.pem").exists());
        let key = std::fs::read(store.join("key.der")).unwrap();
        if run == 0 {
            kept = key;
        } else {
            assert_eq!(key, kept, "a restart made another key");
        }
        let _ = stop.send(true);
    }
}
