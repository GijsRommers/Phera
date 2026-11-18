--TEST--
traits: trait extends error
--FILE--
<?php
trait T{} class C extends T{}
--EXPECTF--
Fatal error: Class C cannot extend trait T in /private%s on line 2
