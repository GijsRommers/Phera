--TEST--
Unsetting a null key removes the empty-string entry without an offset deprecation
--FILE--
<?php
class Box { public array $data = ['' => 1]; }
$b = new Box;
unset($b->data[null]);
var_dump($b->data);
--EXPECT--
array(0) {
}
