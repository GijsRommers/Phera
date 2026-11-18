//! The SQLite file format is the contract: PHP writes/Phera reads, then Phera writes/PHP reads.

use std::process::Command;

#[test]
fn sqlite_files_roundtrip_between_runtimes() {
    let Ok(binary) = std::env::var("PHERA_BIN") else {
        eprintln!("SKIP: set PHERA_BIN for file interoperability");
        return;
    };
    let php = phera_extension_api::testing::oracle::find_php().expect("PHP required");
    let root = std::env::temp_dir().join(format!("phera-sqlite-files-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let writer = root.join("write.php");
    let reader = root.join("read.php");
    std::fs::write(
        &writer,
        r#"<?php
$p=new PDO('sqlite:'.$argv[1]);
$p->exec('CREATE TABLE a(id INTEGER PRIMARY KEY,raw BLOB,n INTEGER,r REAL,t TEXT)');
$s=$p->prepare('INSERT INTO a VALUES(1,?,?,?,?)');
$s->bindValue(1,"\0\xff\xfe",PDO::PARAM_LOB);
$s->bindValue(2,PHP_INT_MIN,PDO::PARAM_INT);$s->bindValue(3,1.25);$s->bindValue(4,'é');
$s->execute();echo 'written';
"#,
    )
    .unwrap();
    std::fs::write(
        &reader,
        r#"<?php
$d=new SQLite3($argv[1],SQLITE3_OPEN_READONLY);
var_dump($d->querySingle('SELECT *,typeof(raw),typeof(n),typeof(r),typeof(t) FROM a',true));
var_dump($d->querySingle('PRAGMA integrity_check'));
"#,
    )
    .unwrap();
    let mut observed = Vec::new();
    for (index, phera_writes) in [false, true].into_iter().enumerate() {
        let database = root.join(format!("{index}.db"));
        let mut write = if phera_writes {
            Command::new(&binary)
        } else {
            let mut command = Command::new(&php);
            command.arg("-n");
            command
        };
        let result = write.arg(&writer).arg(&database).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        assert_eq!(result.stdout, b"written");
        assert!(
            std::fs::read(&database)
                .unwrap()
                .starts_with(b"SQLite format 3\0")
        );
        let mut read = if phera_writes {
            let mut command = Command::new(&php);
            command.arg("-n");
            command
        } else {
            Command::new(&binary)
        };
        let result = read.arg(&reader).arg(&database).output().unwrap();
        assert!(result.status.success());
        observed.push(result.stdout);
    }
    assert_eq!(observed[0], observed[1]);
    eprintln!("PHP writes/Phera reads and Phera writes/PHP reads: byte-identical values");
    std::fs::remove_dir_all(root).unwrap();
}
