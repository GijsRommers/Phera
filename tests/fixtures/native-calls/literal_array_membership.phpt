--TEST--
Literal in_array applies strict identity and the Zend nonnumeric string rule
--FILE--
<?php
$value = '1';
var_dump(in_array($value, [1, '1', 'word'], true));
var_dump(in_array($value, [1, 'word'], true));
var_dump(in_array($value, [1, 'word']));
var_dump(in_array('word', ['word', 'other'], true));
$value = false;
var_dump(in_array($value, ['word', '']));
$value = 'word';
var_dump(in_array($value, ['word', 'other']));
--EXPECT--
bool(true)
bool(false)
bool(true)
bool(true)
bool(true)
bool(true)
