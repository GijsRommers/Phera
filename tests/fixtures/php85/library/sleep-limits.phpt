--TEST--
sleep and usleep accept only amounts the platform can represent
--FILE--
<?php
foreach (['sleep', 'usleep'] as $call) {
    foreach ([-1, 4294967296] as $amount) {
        try { var_dump($call($amount)); }
        catch (ValueError $error) { echo $error->getMessage(), PHP_EOL; }
    }
}
--EXPECT--
sleep(): Argument #1 ($seconds) must be greater than or equal to 0
int(0)
usleep(): Argument #1 ($microseconds) must be greater than or equal to 0
NULL
