--TEST--
Session callbacks use the initialized backing object of a lazy proxy
--FILE--
<?php
class Handler implements SessionHandlerInterface {
    public string $status;
    public function open(string $path, string $name): bool { $this->status .= ':open'; return true; }
    public function close(): bool { $this->status .= ':close'; return true; }
    public function read(string $id): string { $this->status .= ':read'; return ''; }
    public function write(string $id, string $data): bool { $this->status .= ':write'; return true; }
    public function destroy(string $id): bool { return true; }
    public function gc(int $lifetime): int { return 0; }
}
$reflection = new ReflectionClass(Handler::class);
$handler = $reflection->newLazyProxy(function () {
    $backing = new Handler();
    $backing->status = 'init';
    return $backing;
});
session_id('lazy-handler');
ini_set('session.gc_probability', '0');
session_set_save_handler($handler, false);
$before = $reflection->isUninitializedLazyObject($handler);
$started = session_start();
$closed = session_write_close();
var_dump($before, $started, $closed, $reflection->isUninitializedLazyObject($handler), $handler->status);
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(false)
string(26) "init:open:read:write:close"
