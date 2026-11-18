--TEST--
A label can end a function body without a following semicolon
--FILE--
<?php
function work() {
    goto done;
    echo "skipped\n";
    done:
}
var_dump(work());
--EXPECT--
NULL
