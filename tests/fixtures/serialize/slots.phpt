--TEST--
Number PHP references and object references with different slot rules
--FILE--
<?php
$object = new stdClass;
$x = 12;
$y = 17;
$value = [&$x, &$x, $object, $object, &$y, &$y, &$object, &$object];
$encoded = serialize($value);
echo $encoded, "\n";
$result = unserialize($encoded);
$result[0] = 42;
$result[4] = 30;
$result[2]->test = 1;
var_dump($result[1], $result[5], $result[3] === $result[6], $result[7]->test);
?>
--EXPECT--
a:8:{i:0;i:12;i:1;R:2;i:2;O:8:"stdClass":0:{}i:3;r:3;i:4;i:17;i:5;R:5;i:6;R:3;i:7;R:3;}
int(42)
int(30)
bool(true)
int(1)
