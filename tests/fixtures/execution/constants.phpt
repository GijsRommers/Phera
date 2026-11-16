--TEST--
constants
--FILE--
<?php
const A=42;define('B',43);echo A,B,(int)defined('A'),constant('B');
--EXPECT--
4243143