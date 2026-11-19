--TEST--
Nested if statements past PHP's parser stack stop with "memory exhausted"
--FILE--
<?php
// Each nested `if (...) {` takes six entries of PHP's parser stack.
$t = 1;
try {
    eval(str_repeat('if ($t) { ', 2000) . 'echo "deep";' . str_repeat(' }', 2000));
} catch (ParseError $e) {
    echo get_class($e), ': ', $e->getMessage(), "\n";
}
eval(str_repeat('if ($t) { ', 1500) . 'echo "1500 levels\n";' . str_repeat(' }', 1500));
--EXPECT--
ParseError: memory exhausted
1500 levels
