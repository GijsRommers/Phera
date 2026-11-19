<?php
class RedisSessions implements SessionHandlerInterface {
    private Redis $redis;
    public function __construct(private string $prefix) {}
    public function open(string $path, string $name): bool {
        $this->redis = new Redis();
        return $this->redis->connect('127.0.0.1', 16399) && $this->redis->select(14);
    }
    public function close(): bool { return $this->redis->close(); }
    public function read(string $id): string|false {
        return $this->redis->get($this->prefix . $id) ?: '';
    }
    public function write(string $id, string $data): bool {
        return $this->redis->setEx($this->prefix . $id, 120, $data);
    }
    public function destroy(string $id): bool {
        return $this->redis->del($this->prefix . $id) >= 0;
    }
    public function gc(int $max_lifetime): int|false { return 0; }
}

ob_start();
session_set_save_handler(new RedisSessions($argv[1]), false);
ini_set('session.gc_probability', '0');
session_id('redis');
var_dump(session_start());
$_SESSION['value'] = "a\0b";
var_dump(session_write_close());
var_dump(session_start(), bin2hex($_SESSION['value']));
var_dump(session_destroy());
