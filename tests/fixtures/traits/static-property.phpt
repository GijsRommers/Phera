--TEST--
traits: static property
--FILE--
<?php
trait T {public static int $v=7;} class C{use T;} C::$v++; echo C::$v;
--EXPECT--
8
