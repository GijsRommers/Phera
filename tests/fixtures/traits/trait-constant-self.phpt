--TEST--
traits: trait constant self
--FILE--
<?php
trait T{const X=self::Y;} class C{use T; private const Y=8;} echo C::X;
--EXPECT--
8
