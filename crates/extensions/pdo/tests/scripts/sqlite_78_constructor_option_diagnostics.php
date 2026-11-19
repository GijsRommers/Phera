<?php
// PHERA_ORACLE_PHP_SINCE: 80513
// php-src d8d55b75c5d attributes option diagnostics to argument #4 on both entry points.
foreach ([
    [PDO::ATTR_ERRMODE => 3],
    [PDO::ATTR_CASE => 3],
    [PDO::ATTR_STATEMENT_CLASS => 'x'],
] as $options) {
    foreach ([false, true] as $factory) {
        try {
            $factory ? PDO::connect('sqlite::memory:', options: $options)
                : new PDO('sqlite::memory:', options: $options);
        } catch (Throwable $failure) {
            echo get_class($failure), ': ', $failure->getMessage(), "\n";
        }
    }
}
