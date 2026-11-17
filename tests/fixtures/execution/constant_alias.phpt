--TEST--
constant_alias
--FILE--
<?php
namespace X;use const X\A as B;const A=42;echo B;
--EXPECT--
42