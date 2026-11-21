--TEST--
Bitwise operators and shifts on integers give PHP's results in the loop
--FILE--
<?php
function run(int $n): void
{
    $values = [0, 1, -1, 5, -6, PHP_INT_MAX, PHP_INT_MIN, 0x5A5A];
    $shifts = [0, 1, 3, 62, 63, 64, 65, 100];
    for ($round = 0; $round < $n; $round++) {
        $out = [];
        foreach ($values as $a) {
            foreach ([3, -2, 0xFF, PHP_INT_MIN] as $b) {
                $out[] = ($a & $b) . '/' . ($a | $b) . '/' . ($a ^ $b);
            }
            foreach ($shifts as $by) {
                $out[] = ($a << $by) . '/' . ($a >> $by);
            }
            $c = $a;
            $c &= 0xF0;
            $c |= 3;
            $c ^= 1;
            $c <<= 2;
            $c >>= 1;
            $out[] = $c;
            try {
                $out[] = $a << -1;
            } catch (ArithmeticError $e) {
                $out[] = get_class($e) . ': ' . $e->getMessage();
            }
            try {
                $out[] = $a >> -3;
            } catch (ArithmeticError $e) {
                $out[] = $e->getMessage();
            }
        }
        echo md5(implode(' ', $out)), ' ', implode(' ', array_slice($out, 0, 14)), "\n";
    }
}
run(2);
--EXPECT--
b7906b2b5de8a42a59ed131dcc1912d0 0/3/3 0/-2/-2 0/255/255 0/-9223372036854775808/-9223372036854775808 0/0 0/0 0/0 0/0 0/0 0/0 0/0 0/0 4 ArithmeticError: Bit shift by negative number
b7906b2b5de8a42a59ed131dcc1912d0 0/3/3 0/-2/-2 0/255/255 0/-9223372036854775808/-9223372036854775808 0/0 0/0 0/0 0/0 0/0 0/0 0/0 0/0 4 ArithmeticError: Bit shift by negative number
