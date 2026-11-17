--TEST--
invoke callable type
--FILE--
<?php
class Add{function __invoke($a){return $a+4;}}function useIt(callable $f){return $f(3);}var_dump(useIt(new Add));
--EXPECT--
int(7)
