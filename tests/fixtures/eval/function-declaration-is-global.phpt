--TEST--
Function declaration is global
--FILE--
<?php
function define_it() { eval('function made() { return 6; }'); } define_it(); var_dump(made());
--EXPECT--
int(6)
