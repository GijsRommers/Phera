--TEST--
Native named defaults: intrinsic
--FILE--
<?php
$values = ['probe' => 42];
var_dump(extract($values, prefix: 'native'));
var_dump($probe);
--EXPECT--
int(1)
int(42)
