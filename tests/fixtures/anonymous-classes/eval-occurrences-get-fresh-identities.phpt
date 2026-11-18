--TEST--
anonymous-classes: eval occurrences get fresh identities
--FILE--
<?php
function make(){return eval("return new class{};");} var_dump(get_class(make())===get_class(make()));
--EXPECT--
bool(false)
