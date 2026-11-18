--TEST--
A dynamic enum constant lookup reads through a case object
--FILE--
<?php
enum Direction {
    case West;
    public const Label = 'west';
}
$member = 'Label';
var_dump((Direction::West)::{$member});
--EXPECT--
string(4) "west"
