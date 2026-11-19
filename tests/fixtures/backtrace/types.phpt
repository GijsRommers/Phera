--TEST--
Argument errors identify both backtrace parameters
--FILE--
<?php
foreach (['debug_backtrace', 'debug_print_backtrace'] as $function) {
    foreach ([[[]], [0, []], [0, 1, 2]] as $args) {
        try { $function(...$args); } catch (Throwable $error) {
            echo $error::class, ': ', $error->getMessage(), "\n";
        }
    }
}
--EXPECT--
TypeError: debug_backtrace(): Argument #1 ($options) must be of type int, array given
TypeError: debug_backtrace(): Argument #2 ($limit) must be of type int, array given
ArgumentCountError: debug_backtrace() expects at most 2 arguments, 3 given
TypeError: debug_print_backtrace(): Argument #1 ($options) must be of type int, array given
TypeError: debug_print_backtrace(): Argument #2 ($limit) must be of type int, array given
ArgumentCountError: debug_print_backtrace() expects at most 2 arguments, 3 given
