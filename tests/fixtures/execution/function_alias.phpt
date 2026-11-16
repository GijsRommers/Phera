--TEST--
function_alias
--FILE--
<?php
namespace X;use function X\f as g;function f(){echo 42;}g();
--EXPECT--
42