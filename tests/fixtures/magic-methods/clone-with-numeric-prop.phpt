--TEST--
Numeric clone-with keys are property names
--FILE--
<?php
$copy = clone(new stdClass, [12]);
echo $copy->{'0'};
--EXPECT--
12
