--TEST--
An asynchronous PDO SQLite user session handler
--EXTENSIONS--
PDO
--FILE--
<?php
ob_start();
class SqliteSessions implements SessionHandlerInterface {
    private PDO $pdo;
    public function __construct() {
        $this->pdo = new PDO("sqlite::memory:");
        $this->pdo->exec("CREATE TABLE sessions (id TEXT PRIMARY KEY, data BLOB NOT NULL)");
    }
    public function open(string $path, string $name): bool { return true; }
    public function close(): bool { return true; }
    public function read(string $id): string|false {
        $query = $this->pdo->prepare("SELECT data FROM sessions WHERE id = ?");
        $query->execute([$id]);
        return $query->fetchColumn() ?: "";
    }
    public function write(string $id, string $data): bool {
        return $this->pdo->prepare("INSERT OR REPLACE INTO sessions VALUES (?, ?)")
            ->execute([$id, $data]);
    }
    public function destroy(string $id): bool {
        return $this->pdo->prepare("DELETE FROM sessions WHERE id = ?")->execute([$id]);
    }
    public function gc(int $max_lifetime): int|false { return 0; }
}
session_set_save_handler(new SqliteSessions(), false);
ini_set("session.gc_probability", 0);
session_id("sqlite");
session_start();
$_SESSION["message"] = "PDO";
session_write_close();
session_start();
var_dump($_SESSION, session_destroy());
?>
--EXPECT--
array(1) {
  ["message"]=>
  string(3) "PDO"
}
bool(true)
