--TEST--
traits: declaration
--FILE--
<?php
trait T {} var_dump(trait_exists("T"));
--EXPECT--
bool(true)
