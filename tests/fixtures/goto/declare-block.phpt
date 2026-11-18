--TEST--
A declare block shares the enclosing label table
--FILE--
<?php
goto inside;
declare(ticks=0) {
    echo "skipped\n";
    inside: echo "inside\n";
    goto outside;
}
outside:
echo "outside\n";
--EXPECT--
inside
outside
