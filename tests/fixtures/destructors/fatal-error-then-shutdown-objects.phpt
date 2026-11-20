--TEST--
fatal-error-then-shutdown-objects
--FILE--
<?php
// After a fatal error no object alive at that moment is destructed, but
// temporaries in shutdown functions are, and so are objects they keep.
class T { function __construct(public $n) {} function __destruct() { echo "destruct {$this->n}\n"; } function name() { return $this->n; } }
$alive = new T('alive before the fatal error');
register_shutdown_function(function () {
    echo "shutdown starts\n";
    echo "name: ", (new T('temporary'))->name(), "\n";
    $local = new T('local');
    unset($local);
    $GLOBALS['kept'] = new T('kept by shutdown');
    echo "shutdown ends\n";
});
eval('function twice() {}');
eval('function twice() {}');
echo "not reached\n";
--EXPECTF--

Fatal error: Cannot redeclare function twice() (previously declared in %s(14) : eval()'d code:1) in %s(15) : eval()'d code on line 1
shutdown starts
name: destruct temporary
temporary
destruct local
shutdown ends
destruct kept by shutdown
