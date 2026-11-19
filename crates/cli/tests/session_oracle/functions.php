<?php
ob_start();
session_save_path($argv[1]);
ini_set('session.gc_probability', 0);
ini_set('session.use_cookies', 0);
session_cache_limiter('');

function probe(string $function, array $arguments): void {
    $diagnostics = [];
    set_error_handler(function ($level, $message) use (&$diagnostics) {
        $diagnostics[] = [$level, $message];
        return true;
    });
    try {
        $value = $function(...$arguments);
        if ($function === 'session_create_id' && is_string($value)) {
            $prefix = substr($value, 0, -32);
            $value = [$prefix, preg_match('/^[0-9a-f]{32}$/D', substr($value, -32))];
        }
        if ($function === 'session_save_path' && $value === $GLOBALS['argv'][1]) {
            $value = '<save path>';
        }
        $value = ['return', bin2hex(serialize($value))];
    } catch (Throwable $error) {
        $value = [$error::class, $error->getMessage()];
    } finally {
        restore_error_handler();
    }
    echo json_encode([$function, $value, $diagnostics], JSON_INVALID_UTF8_SUBSTITUTE), "\n";
}

$inputs = [null, true, false, 0, 1, -1, 2, 1440, PHP_INT_MAX, PHP_INT_MIN,
    2.5, '', 'yes', '0', 'php', 'php_binary', 'php_serialize', '-2K',
    '0x20', '16K', ' 12 ', '00', 'some', "bad\0tail", "true\0tail", 4294967296, 'bad.path', '/path',
    'a,b', '31X', []];

foreach (['session_name', 'session_save_path', 'session_module_name', 'session_id',
    'session_cache_limiter', 'session_cache_expire', 'session_get_cookie_params',
    'session_set_save_handler', 'session_decode'] as $function) {
    probe($function, []);
    foreach ($inputs as $value) {
        probe($function, [$value]);
    }
}

foreach (['a', 'z', 'A', 'Z', '0', '9', '-', ',', '_', '/', '.', "\0", "\xff", 'é'] as $byte) {
    foreach ([0, 1, 2, 15, 16, 31, 32, 63, 127, 128, 129, 254, 255, 256, 257] as $n) {
        probe('session_create_id', [str_repeat($byte, $n)]);
    }
}

$settings = ['name', 'cache_limiter', 'serialize_handler', 'use_cookies', 'use_strict_mode',
    'cookie_lifetime', 'cookie_path', 'cookie_domain', 'cookie_secure', 'cookie_partitioned',
    'cookie_httponly', 'cookie_samesite', 'gc_probability', 'gc_divisor',
    'gc_maxlifetime', 'cache_expire', 'lazy_write', 'save_handler', 'save_path', 'auto_start'];
foreach ($settings as $setting) {
    $name = 'session.' . $setting;
    $original = ini_get($name);
    foreach ($inputs as $value) {
        probe('ini_set', [$name, $value]);
        probe('ini_get', [$name]);
    }
    ini_set($name, $original);
}

ini_set('session.use_cookies', 1);
foreach (['lifetime', 'path', 'domain', 'secure', 'partitioned', 'httponly', 'samesite', 'unknown'] as $key) {
    foreach ($inputs as $value) {
        probe('session_set_cookie_params', [[$key => $value]]);
    }
}
foreach ($inputs as $value) {
    probe('session_set_cookie_params', [$value]);
}
session_set_cookie_params(0, '/', '', false, false);
session_set_cookie_params(['partitioned' => false, 'samesite' => '']);
ini_set('session.use_cookies', 0);

class EmptySessions implements SessionHandlerInterface {
    public function open(string $path, string $name): bool { return true; }
    public function close(): bool { return true; }
    public function read(string $id): string|false { return 'x|i:1;'; }
    public function write(string $id, string $data): bool { return true; }
    public function destroy(string $id): bool { return true; }
    public function gc(int $max_lifetime): int|false { return 3; }
}
session_name('PHPSESSID');
session_save_path($argv[1]);
ini_set('session.serialize_handler', 'php');
session_cache_limiter('');
session_set_save_handler(new EmptySessions(), false);
session_id('matrix');
foreach (['read_and_close', 'unknown', 'cache_expire', 'gc_divisor', 'lazy_write'] as $option) {
    foreach ($inputs as $value) {
        probe('session_start', [[$option => $value]]);
        session_abort();
    }
}
foreach ([false, true] as $start) {
    foreach (['session_status', 'session_abort', 'session_commit', 'session_write_close',
        'session_destroy', 'session_gc', 'session_reset', 'session_unset', 'session_encode',
        'session_regenerate_id', 'session_register_shutdown'] as $function) {
        if ($start) session_start();
        probe($function, []);
        session_abort();
        foreach ($inputs as $value) {
            probe($function, [$value]);
        }
    }
}
