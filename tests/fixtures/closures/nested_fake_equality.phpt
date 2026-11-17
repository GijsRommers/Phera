--TEST--
nested fake equality
--FILE--
<?php
$a=strlen(...);$b=strlen(...);var_dump([$a]==[$b],[$a]===[$b],(object)["f"=>$a]==(object)["f"=>$b]);
--EXPECT--
bool(true)
bool(false)
bool(true)
