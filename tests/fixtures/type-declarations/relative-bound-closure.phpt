--TEST--
A closure can bind the declaring class of a relative type
--FILE--
<?php
class Receiver {}
$callback = fn(self $value) => get_debug_type($value);
$callback = Closure::bind($callback, null, Receiver::class);
echo $callback(new Receiver()), "\n";
--EXPECT--
Receiver
