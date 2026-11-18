--TEST--
Namespace is not inherited
--FILE--
<?php
namespace Outer; eval('function made() {}'); var_dump(function_exists('made'), function_exists('Outer\made'));
--EXPECT--
bool(true)
bool(false)
