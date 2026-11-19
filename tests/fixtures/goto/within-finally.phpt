--TEST--
A jump inside a finally block keeps the original return
--FILE--
<?php
function work() {
    try {
        return 7;
    } finally {
        goto end;
        echo "skipped\n";
        end:
    }
}
var_dump(work());
--EXPECT--
int(7)
