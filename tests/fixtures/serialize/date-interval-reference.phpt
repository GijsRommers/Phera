--TEST--
DateInterval uses the default for a serialized reference in an integer field
--FILE--
<?php
$state = unserialize('a:2:{i:0;O:12:"DateInterval":1:{s:1:"y";R:1;}i:1;i:2;}');
var_dump($state[0]->y, $state[1]);
?>
--EXPECT--
int(-1)
int(2)
