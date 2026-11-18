--TEST--
traits: matching untyped null property
--FILE--
<?php
trait T {public $p;} class C{use T; public $p=null;} var_dump((new C)->p);
--EXPECT--
NULL
