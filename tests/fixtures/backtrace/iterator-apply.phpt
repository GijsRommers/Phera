--TEST--
iterator_apply gives the callback an internal frame
--FILE--
<?php
iterator_apply(new ArrayIterator([12]), function () {
    debug_print_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS);
    return false;
});
--EXPECTF--
#0 [internal function]: {closure:%s/iterator-apply.php:2}()
#1 %s/iterator-apply.php(2): iterator_apply()
