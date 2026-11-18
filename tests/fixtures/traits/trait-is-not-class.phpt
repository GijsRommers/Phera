--TEST--
traits: trait is not class
--FILE--
<?php
trait T{} var_dump(class_exists(T::class), interface_exists(T::class));
--EXPECT--
bool(false)
bool(false)
