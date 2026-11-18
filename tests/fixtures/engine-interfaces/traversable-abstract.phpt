--TEST--
traversable abstract
--FILE--
<?php

abstract class Base implements Traversable {} var_dump(interface_exists("Traversable"));
--EXPECT--
bool(true)
