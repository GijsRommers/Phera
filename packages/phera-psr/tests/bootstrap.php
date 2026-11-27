<?php

declare(strict_types=1);

// Loads the adapters and the PSR interfaces: from vendor/ after
// `composer install`, otherwise from tests/stubs so the tests run offline.

$vendor = dirname(__DIR__) . '/vendor/autoload.php';
if (is_file($vendor)) {
    require $vendor;
} else {
    foreach (['http-message', 'http-factory', 'http-server', 'http-client', 'log'] as $stub) {
        require_once __DIR__ . "/stubs/$stub.php";
    }
    spl_autoload_register(static function (string $class): void {
        if (str_starts_with($class, 'Phera\\Psr\\')) {
            $file = dirname(__DIR__) . '/src/' . str_replace('\\', '/', substr($class, 10)) . '.php';
            if (is_file($file)) {
                require $file;
            }
        }
    });
}

/** Counts checks; a failed one is printed and makes the script exit 1 at the end. */
final class Check
{
    public static int $passed = 0;

    /** @var list<string> */
    public static array $failed = [];

    public static function same(mixed $expected, mixed $actual, string $label): void
    {
        if ($expected === $actual) {
            self::$passed++;
            return;
        }
        self::$failed[] = sprintf("%s: expected %s, got %s", $label, var_export($expected, true), var_export($actual, true));
    }

    public static function true(bool $condition, string $label): void
    {
        self::same(true, $condition, $label);
    }

    /** @param class-string<\Throwable> $class */
    public static function throws(string $class, callable $code, string $label): void
    {
        try {
            $code();
        } catch (\Throwable $thrown) {
            self::same($class, $thrown instanceof $class ? $class : $thrown::class, $label);
            return;
        }
        self::$failed[] = "$label: expected $class, nothing thrown";
    }

    public static function report(string $suite): never
    {
        foreach (self::$failed as $failure) {
            fwrite(STDERR, "FAIL $failure\n");
        }
        echo $suite, ': ', self::$passed, ' passed, ', count(self::$failed), " failed\n";
        exit(self::$failed === [] ? 0 : 1);
    }
}
