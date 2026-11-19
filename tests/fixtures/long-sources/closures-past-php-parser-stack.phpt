--TEST--
Closures nested past PHP's parser stack stop with "memory exhausted"
--FILE--
<?php
// PHP's parser stack holds 10,000 entries; each closure level takes twelve.
try {
    eval('return ' . str_repeat('function () { return ', 900) . '1' . str_repeat('; }', 900) . ';');
} catch (ParseError $e) {
    echo get_class($e), ': ', $e->getMessage(), ' on line ', $e->getLine(), "\n";
}
$f = eval('return ' . str_repeat('function () { return ', 800) . '1' . str_repeat('; }', 800) . ';');
echo get_class($f), "\n";
--EXPECT--
ParseError: memory exhausted on line 1
Closure
