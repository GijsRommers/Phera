--TEST--
Labels in alternative declare syntax do not consume endif-style colons
--FILE--
<?php
goto inside;
declare(ticks=0):
    inside: echo "inside\n";
enddeclare;
--EXPECT--
inside
