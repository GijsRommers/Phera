--TEST--
A session callback sees its caller's current fiber
--FILE--
<?php
class Handler implements SessionHandlerInterface {
    public ?Fiber $current = null;
    public function open(string $path, string $name): bool { $this->current = Fiber::getCurrent(); return true; }
    public function close(): bool { return true; }
    public function read(string $id): string { return ''; }
    public function write(string $id, string $data): bool { return true; }
    public function destroy(string $id): bool { return true; }
    public function gc(int $lifetime): int { return 0; }
}
$handler = new Handler();
session_id('fiber-handler');
ini_set('session.gc_probability', '0');
session_set_save_handler($handler, false);
$fiber = new Fiber(function () {
    session_start();
    session_abort();
});
$fiber->start();
var_dump($handler->current === $fiber, Fiber::getCurrent());
--EXPECT--
bool(true)
NULL
