--TEST--
A non-string constant name on a linked class raises TypeError
--FILE--
<?php
class Present {}
$member = 0;
try {
    Present::{$member};
} catch (Throwable $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
--EXPECT--
TypeError: Cannot use value of type int as class constant name
