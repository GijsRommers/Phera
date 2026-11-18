--TEST--
A string cast can change a later property in the same comparison
--FILE--
<?php
class Row { public $a; public $b = 'before'; }
class Text {
    function __construct(public Row $row) {}
    function __toString() { $this->row->b = 'after'; return 'a'; }
}
$left = new Row;
$right = new Row;
$left->a = new Text($left);
$right->a = 'a';
$right->b = 'after';
var_dump($left == $right);
--EXPECT--
bool(true)
