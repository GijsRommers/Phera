--TEST--
Unserialize maps older property spellings to inherited private and public declarations
--FILE--
<?php
class ParentData { private $x = 1; }
class ChildData extends ParentData { public $y = 'keep'; }
$public = 'O:9:"ChildData":1:{s:1:"x";i:2;}';
var_dump(bin2hex(serialize(unserialize($public))));
class Exposed { public $x; }
$private = "O:7:\"Exposed\":1:{s:10:\"\0Exposed\0x\";i:3;}";
var_dump(unserialize($private)->x);
--EXPECT--
string(128) "4f3a393a224368696c6444617461223a323a7b733a31333a2200506172656e74446174610078223b693a323b733a313a2279223b733a343a226b656570223b7d"
int(3)
