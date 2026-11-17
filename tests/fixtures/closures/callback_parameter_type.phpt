--TEST--
callback parameter type
--FILE--
<?php
function useCallback(callable $f){return $f(3);}var_dump(useCallback(fn($x)=>$x+2));try{useCallback("missingClosureFixture");}catch(TypeError){echo "invalid callback
";}
--EXPECT--
int(5)
invalid callback
