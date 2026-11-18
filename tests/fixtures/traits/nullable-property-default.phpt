--TEST--
traits: nullable property default
--FILE--
<?php
trait T {public ?string $v = null;} class C{use T;} var_dump((new C)->v);
--EXPECT--
NULL
