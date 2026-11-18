--TEST--
A dynamically fetched enum case is the existing singleton
--FILE--
<?php
enum Direction { case West; }
$member = 'West';
var_dump(Direction::{$member} === Direction::West);
--EXPECT--
bool(true)
