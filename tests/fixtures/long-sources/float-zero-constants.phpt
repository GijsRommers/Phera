--TEST--
0.0 and -0.0 written in one file stay two different constants
--FILE--
<?php
var_dump(0.0, -0.0, 0.0);
$values = [0.0, -0.0];
var_dump($values[1]);
echo json_encode([-0.0, 0.0]), "\n";
--EXPECT--
float(0)
float(-0)
float(0)
float(-0)
[-0,0]
