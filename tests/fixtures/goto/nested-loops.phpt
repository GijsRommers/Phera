--TEST--
A forward goto leaves nested loops and a switch
--FILE--
<?php
foreach ([1, 2] as $outer) {
    for ($inner = 1; $inner < 3; ++$inner) {
        switch ($inner) {
            case 1: echo "$outer.$inner\n"; break;
            case 2: goto matched;
        }
    }
}
echo "miss\n";
matched:
echo "matched\n";
--EXPECT--
1.1
matched
