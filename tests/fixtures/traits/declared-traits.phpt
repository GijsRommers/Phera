--TEST--
traits: declared traits
--FILE--
<?php
trait T{} trait U{} var_export(get_declared_traits());
--EXPECT--
array (
  0 => 'T',
  1 => 'U',
)
