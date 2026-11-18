--TEST--
traits: equivalent union default types
--FILE--
<?php
trait T{public int|string $a;} class C{use T; public string|int $a;} echo "compatible";
--EXPECT--
compatible
