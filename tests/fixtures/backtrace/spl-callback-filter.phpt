--TEST--
CallbackFilterIterator shows both its accept and the native rewind
--FILE--
<?php
$iterator = new CallbackFilterIterator(new ArrayIterator([12]), function () {
    debug_print_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS);
    $trace = debug_backtrace();
    echo get_class($trace[1]['object']), '/', get_class($trace[2]['object']), "\n";
    return true;
});
foreach ($iterator as $item) { break; }
--EXPECTF--
#0 [internal function]: {closure:%s/spl-callback-filter.php:2}()
#1 [internal function]: CallbackFilterIterator->accept()
#2 %s/spl-callback-filter.php(8): FilterIterator->rewind()
CallbackFilterIterator/CallbackFilterIterator
