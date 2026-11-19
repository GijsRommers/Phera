--TEST--
PHP session objects publish data in serializer-specific order
--FILE--
<?php
ob_start();
class SessionValue {
    public function __wakeup(): void {
        echo "wakeup ", isset($_SESSION["object"]) ? "visible" : "hidden", "\n";
        throw new RuntimeException("session wakeup");
    }
}
class HookSessions implements SessionHandlerInterface {
    public function __construct(public string $data) {}
    public function open(string $path, string $name): bool { return true; }
    public function close(): bool { echo "close\n"; return true; }
    public function read(string $id): string|false { return $this->data; }
    public function write(string $id, string $data): bool { return true; }
    public function destroy(string $id): bool { return true; }
    public function gc(int $max_lifetime): int|false { return 0; }
}
ini_set("session.gc_probability", 0);
foreach (["php", "php_serialize", "php_binary"] as $format) {
    ini_set("session.serialize_handler", $format);
    $serialized = serialize(new SessionValue());
    $data = match ($format) {
        "php" => "object|".$serialized,
        "php_binary" => "\x06object".$serialized,
        default => serialize(["object" => new SessionValue()]),
    };
    session_set_save_handler(new HookSessions($data), false);
    session_id("hooks");
    try { session_start(); }
    catch (RuntimeException $exception) { echo $exception->getMessage(), "\n"; }
    var_dump(session_status(), $_SESSION["object"] instanceof SessionValue);
    session_abort();
}
?>
--EXPECT--
wakeup visible
session wakeup
int(2)
bool(true)
close
wakeup hidden
session wakeup
int(2)
bool(true)
close
wakeup visible
session wakeup
int(2)
bool(true)
close
