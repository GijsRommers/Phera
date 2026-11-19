--TEST--
ArrayObject shows the internal sort and the method that invoked it
--FILE--
<?php
$values = new ArrayObject([2, 1]);
$values->uasort(function ($a, $b) {
    debug_print_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS);
    echo get_class(debug_backtrace()[2]['object']), "\n";
    return $a <=> $b;
});
--EXPECTF--
#0 [internal function]: {closure:%s/spl-uasort.php:3}()
#1 [internal function]: uasort()
#2 %s/spl-uasort.php(3): ArrayObject->uasort()
ArrayObject
