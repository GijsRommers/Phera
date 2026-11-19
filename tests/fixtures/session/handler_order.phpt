--TEST--
User handlers and optional callbacks in lifecycle order
--FILE--
<?php
ob_start();
class MemoryHandler implements SessionHandlerInterface, SessionIdInterface, SessionUpdateTimestampHandlerInterface {
    private array $rows = [];
    private int $next = 0;
    public function open(string $path, string $name): bool { echo "open:$name\n"; return true; }
    public function close(): bool { echo "close\n"; return true; }
    public function read(string $id): string|false { echo "read:$id\n"; return $this->rows[$id] ?? ""; }
    public function write(string $id, string $data): bool {
        echo "write:$id:$data\n";
        $this->rows[$id] = $data;
        return true;
    }
    public function destroy(string $id): bool { echo "destroy:$id\n"; unset($this->rows[$id]); return true; }
    public function gc(int $max_lifetime): int|false { echo "gc:$max_lifetime\n"; return 7; }
    public function create_sid(): string { return "memory-" . ++$this->next; }
    public function validateId(string $id): bool { echo "validate:$id\n"; return isset($this->rows[$id]); }
    public function updateTimestamp(string $id, string $data): bool { echo "touch:$id:$data\n"; return true; }
}
var_dump(session_set_save_handler(new MemoryHandler(), false), session_module_name());
ini_set("session.gc_probability", 1);
ini_set("session.gc_divisor", 1);
ini_set("session.use_strict_mode", 1);
session_id("unknown");
var_dump(session_start(), session_id(), session_gc());
$_SESSION["count"] = 1;
var_dump(session_write_close());
var_dump(session_start());
var_dump(session_write_close());
var_dump(session_start());
var_dump(session_regenerate_id(true), session_id(), $_SESSION);
var_dump(session_destroy());
?>
--EXPECT--
bool(true)
string(4) "user"
open:PHPSESSID
validate:unknown
read:memory-1
gc:1440
gc:1440
bool(true)
string(8) "memory-1"
int(7)
write:memory-1:count|i:1;
close
bool(true)
open:PHPSESSID
validate:memory-1
read:memory-1
gc:1440
bool(true)
touch:memory-1:count|i:1;
close
bool(true)
open:PHPSESSID
validate:memory-1
read:memory-1
gc:1440
bool(true)
destroy:memory-1
close
open:PHPSESSID
validate:memory-2
read:memory-2
bool(true)
string(8) "memory-2"
array(1) {
  ["count"]=>
  int(1)
}
destroy:memory-2
close
bool(true)
