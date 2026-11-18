--TEST--
anonymous-classes: static per site
--FILE--
<?php
function make() { return new class { static int $count = 0; function increment() { return ++self::$count; } }; } echo make()->increment(), make()->increment();
--EXPECT--
12
