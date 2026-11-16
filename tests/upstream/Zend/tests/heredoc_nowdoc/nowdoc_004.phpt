--TEST--
braces variable replacement test (nowdoc)
--FILE--
<?php

require_once 'nowdoc.inc';

print <<<'ENDOFNOWDOC'
This is nowdoc test #{$a}.

ENDOFNOWDOC;

$x = <<<'ENDOFNOWDOC'
This is nowdoc test #{$b}.

ENDOFNOWDOC;

print "{$x}";

?>
--EXPECT--
This is nowdoc test #{$a}.
This is nowdoc test #{$b}.
--XFAIL--
Requires classes and object construction in included nowdoc.inc (outside this milestone).
