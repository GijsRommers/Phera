--TEST--
anonymous-classes: constructor named argument
--FILE--
<?php
new class(b: 7, a: 1) { function __construct($a, $b) { echo $a, $b; } };
--EXPECT--
17
