--TEST--
traits: matching property
--FILE--
<?php
trait T {public int $p=1;} class C{use T; public int $p=1;} echo (new C)->p;
--EXPECT--
1
