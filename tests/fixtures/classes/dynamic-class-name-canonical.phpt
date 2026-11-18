--TEST--
A dynamic class pseudo-constant returns the linked class name through an alias
--FILE--
<?php
class OriginalName {}
class_alias(OriginalName::class, 'OtherName');
$member = strtolower('CLASS');
var_dump(OtherName::{$member});
--EXPECT--
string(12) "OriginalName"
