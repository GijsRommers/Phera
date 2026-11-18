--TEST--
Class autoloading precedes dynamic constant name validation
--FILE--
<?php
spl_autoload_register(function (string $name): void {
    echo $name, "\n";
    class Loaded {}
});
$member = 0;
try {
    Loaded::{$member};
} catch (Throwable $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
--EXPECT--
Loaded
TypeError: Cannot use value of type int as class constant name
