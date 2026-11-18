--TEST--
Foreach visits the array of enum singleton cases in declaration order
--FILE--
<?php
enum Direction { case West; case East; }
foreach (Direction::cases() as $direction) {
    echo $direction->name, ':', (int) ($direction === Direction::West), "\n";
}
--EXPECT--
West:1
East:0
