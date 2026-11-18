<?php
global $argv, $r, $p, $checks;
$r = new Redis();
$r->connect('127.0.0.1', 16399);
$r->select(14);
$p = $argv[1];
if (!$p) { throw new RuntimeException('A unique prefix argument is required'); }
$keys = $r->keys($p . '*');
if ($keys) { $r->del($keys); }
$checks = 0;
function check($f) {
    global $checks;
    $checks++;
    try {
        $value = $f();
        if ($value instanceof Redis) { echo "Redis\n"; }
        else { var_dump($value); }
    } catch (Throwable $e) {
        echo get_class($e), ': ', $e->getMessage(), "\n";
    }
}
function finish() {
    global $r, $p, $checks;
    $r->discard();
    $r->setOption(Redis::OPT_PREFIX, '');
    $r->setOption(Redis::OPT_SERIALIZER, Redis::SERIALIZER_NONE);
    $keys = $r->keys($p . '*');
    if ($keys) { $r->del($keys); }
    echo 'CALLS=', $checks, "\n";
}
