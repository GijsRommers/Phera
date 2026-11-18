--TEST--
Class declaration is global
--FILE--
<?php
function define_it() { eval('class Made {}'); } define_it(); var_dump(class_exists('Made',false));
--EXPECT--
bool(true)
