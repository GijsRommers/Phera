--TEST--
Special strlen still checks the calling file strictness
--FILE--
<?php
declare(strict_types=1);
function length($value) { return strlen($value); }
try {
    length(42);
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
strlen(): Argument #1 ($string) must be of type string, int given
