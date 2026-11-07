use rusqlite::Connection;
fn main() {
    let path = std::env::temp_dir().join("washi_spike.db");
    let _ = std::fs::remove_file(&path);
    let c = Connection::open(&path).unwrap();
    c.pragma_update(None, "key", "x'2DD29CA851E7B56E4697B0E1F08507293D761A05CE4D1B628663F411A8086D99'").unwrap();
    c.execute_batch("CREATE TABLE n(id INTEGER PRIMARY KEY, t TEXT); INSERT INTO n(t) VALUES ('hello washi');").unwrap();
    let v: String = c.query_row("SELECT cipher_version()", [], |r| r.get(0)).unwrap();
    drop(c);
    let raw = std::fs::read(&path).unwrap();
    let plain = raw.windows(11).any(|w| w == b"hello washi");
    println!("cipher_version={v} file_contains_plaintext={plain} header_is_sqlite={}", raw.starts_with(b"SQLite format 3"));
    let c = Connection::open(&path).unwrap();
    println!("open_without_key_fails={}", c.query_row("SELECT count(*) FROM n", [], |r| r.get::<_, i64>(0)).is_err());
}
