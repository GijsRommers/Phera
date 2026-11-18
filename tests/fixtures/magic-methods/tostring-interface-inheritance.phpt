--TEST--
tostring interface inheritance
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
class Child extends Text {} var_dump(new Child instanceof Stringable);
--EXPECT--
bool(true)
