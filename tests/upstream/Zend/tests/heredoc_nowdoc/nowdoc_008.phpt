--TEST--
empty doc test (nowdoc)
--FILE--
<?php

require_once 'nowdoc.inc';

print <<<'ENDOFNOWDOC'
ENDOFNOWDOC;

$x = <<<'ENDOFNOWDOC'
ENDOFNOWDOC;

print "{$x}";

?>
--EXPECT--
--XFAIL--
Requires classes and object construction in included nowdoc.inc (outside this milestone).
