--TEST--
eval converts its object argument to source bytes
--FILE--
<?php
class Source { function __toString() { return 'return 42;'; } } var_dump(eval(new Source));
--EXPECT--
int(42)
