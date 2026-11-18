--TEST--
Argument type alias
--FILE--
<?php
class Original {} class_alias("Original", "Another"); function accept(Another $value) { echo get_class($value); } accept(new Original);
--EXPECT--
Original