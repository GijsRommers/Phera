--TEST--
Round validates invalid mode values and union types
--FILE--
<?php
foreach ([9, [], new stdClass()] as $mode) {
    try { round(1.5, mode: $mode); } catch (Throwable $error) {
        echo $error::class, ': ', $error->getMessage(), "\n";
    }
}
--EXPECT--
ValueError: round(): Argument #3 ($mode) must be a valid rounding mode (RoundingMode::*)
TypeError: round(): Argument #3 ($mode) must be of type RoundingMode|int, array given
TypeError: round(): Argument #3 ($mode) must be of type RoundingMode|int, stdClass given
