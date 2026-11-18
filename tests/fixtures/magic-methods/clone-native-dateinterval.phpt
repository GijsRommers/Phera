--TEST--
Clone-with changes native virtual fields on the new DateInterval only
--FILE--
<?php
$original = new DateInterval('P1D');
$copy = clone($original, ["d" => 5]);
echo $original->format('%d'), '/', $copy->format('%d');
--EXPECT--
1/5
