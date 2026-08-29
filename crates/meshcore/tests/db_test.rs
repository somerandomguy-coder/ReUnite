use std::collections::HashMap;
use meshcore::identity::Identity;
use meshcore::net::default_network_id;
use meshcore::store::{DatabaseStore, Contact, StoredMessage, REUNITE_SQL_SCHEMA};
use meshcore::types::now_ms;

#[test]
fn test_database_store_lifecycle() {
    let tmp_dir = tempfile::tempdir().expect("create temp dir");
    let db = DatabaseStore::new(tmp_dir.path().to_path_buf()).expect("init db store");

    // 1. Verify SQL DDL schema export
    assert!(db.sql_schema().contains("CREATE TABLE IF NOT EXISTS contacts"));
    assert!(db.sql_schema().contains("CREATE TABLE IF NOT EXISTS messages"));
    assert!(db.sql_schema().contains("CREATE TABLE IF NOT EXISTS networks"));
    assert!(db.sql_schema().contains("CREATE TABLE IF NOT EXISTS safe_zones"));
    assert!(db.sql_schema().contains("CREATE TABLE IF NOT EXISTS breadcrumbs"));
    assert!(!REUNITE_SQL_SCHEMA.is_empty());

    // 2. Test Contact saving and reloading
    let id_a = Identity::load_or_create(tmp_dir.path()).expect("generate identity").id;
    let mut contacts = HashMap::new();
    contacts.insert(
        id_a,
        Contact {
            id: id_a,
            ed_pub: [1u8; 32],
            x_pub: [2u8; 32],
            alias: Some("Alice".to_string()),
            self_name: Some("Alice-Device".to_string()),
            last_seen_ms: now_ms(),
            gps: None,
            battery: Some(88),
            status: Some(1),
            sos: false,
        },
    );

    db.save_contacts(&contacts).expect("save contacts");
    let reloaded = db.load_contacts().expect("load contacts");
    assert_eq!(reloaded.len(), 1);
    assert_eq!(reloaded.get(&id_a).unwrap().alias.as_deref(), Some("Alice"));

    // 3. Test Message Log persistence
    let net_id = default_network_id();
    let msg = StoredMessage {
        ts_ms: now_ms(),
        network: net_id.to_hex(),
        network_name: "default".to_string(),
        kind: "chat".to_string(),
        from: "Alice".to_string(),
        to: None,
        text: "Emergency test message over DB".to_string(),
    };

    db.append_msg(&net_id, &msg).expect("append msg");
    let msgs = db.get_messages(&net_id, 10).expect("read msgs");
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].text, "Emergency test message over DB");
}
