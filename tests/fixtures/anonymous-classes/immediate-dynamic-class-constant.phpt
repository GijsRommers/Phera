--TEST--
anonymous-classes: immediate dynamic class constant
--FILE--
<?php
echo new class { const C = 12; }::{"C"};
--EXPECT--
12
