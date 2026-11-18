--TEST--
traits: case insensitive trait name
--FILE--
<?php
trait Example {} class C{use eXaMpLe;} var_dump(trait_exists("example"));
--EXPECT--
bool(true)
