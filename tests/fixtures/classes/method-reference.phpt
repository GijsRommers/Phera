--TEST--
Classes: method-reference
--FILE--
<?php class Box { function update(&$value) { $value++; } } $value=8; (new Box)->update($value); echo $value;
--EXPECT--
9