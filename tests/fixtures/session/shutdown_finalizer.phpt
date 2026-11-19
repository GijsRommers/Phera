--TEST--
An implicit session close sees changes from shutdown and output callbacks
--FILE--
<?php
class ShutdownSession implements SessionHandlerInterface {
    public function open(string $path, string $name): bool { return true; }
    public function close(): bool { echo "close\n"; return true; }
    public function read(string $id): string|false { return ""; }
    public function write(string $id, string $data): bool { echo "write:$data\n"; return true; }
    public function destroy(string $id): bool { return true; }
    public function gc(int $max_lifetime): int|false { return 0; }
}
session_set_save_handler(new ShutdownSession(), false);
session_start();
register_shutdown_function(function () {
    echo "callback ", session_status(), "\n";
    $_SESSION["done"] = true;
});
ob_start(function (string $buffer): string {
    $_SESSION["flush"] = "buffer";
    return $buffer;
});
echo "script\n";
?>
--EXPECT--
script
callback 2
write:done|b:1;flush|s:6:"buffer";
close
