--TEST--
A property write evaluates the receiver and keys before its source expression
--FILE--
<?php
class Items { function __get($name) { echo 'get'; return []; } }
function items() { echo 'receiver '; return new Items; }
function index() { echo 'index '; return 0; }
function value() { echo 'value '; return 1; }
items()->entries[index()] = value();
--EXPECTF--
receiver index value get
Notice: Indirect modification of overloaded property Items::$entries has no effect in %s on line 6
