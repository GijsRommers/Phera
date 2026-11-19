--TEST--
Special count and sizeof invoke the Countable method
--FILE--
<?php
class Tally implements Countable {
    public function count(): int {
        echo "count called\n";
        return 41;
    }
}
$values = new Tally();
var_dump(count($values), sizeof($values));
--EXPECT--
count called
count called
int(41)
int(41)
