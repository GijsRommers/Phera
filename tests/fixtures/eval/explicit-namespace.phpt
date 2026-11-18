--TEST--
Explicit namespace
--FILE--
<?php
eval('namespace Made; class C {}'); var_dump(class_exists('Made\C',false));
--EXPECT--
bool(true)
