--TEST--
Restore cycles across arrays, scalar aliases and objects
--FILE--
<?php
$node = new stdClass;
$node->self = $node;
$s = serialize([$node, $node]);
echo $s, "\n";
$v = unserialize($s);
var_dump($v[0]->self === $v[1]);
$loop = unserialize('a:2:{s:4:"self";R:1;s:5:"value";i:1;}');
var_dump($loop['self']['value']);
?>
--EXPECT--
a:2:{i:0;O:8:"stdClass":1:{s:4:"self";r:2;}i:1;r:2;}
bool(true)
int(1)
