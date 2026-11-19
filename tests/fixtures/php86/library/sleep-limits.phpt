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
sleep(): Argument #1 ($seconds) must be between 0 and 4294967295
sleep(): Argument #1 ($seconds) must be between 0 and 4294967295
usleep(): Argument #1 ($microseconds) must be between 0 and 4294967295
usleep(): Argument #1 ($microseconds) must be between 0 and 4294967295
