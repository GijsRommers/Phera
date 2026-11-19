--TEST--
shutdown-callback-temporaries
--FILE--
<?php
// Temporaries and locals released inside shutdown functions are destructed
// at once, as anywhere else (reported by the execution time limit work).
class T { function __construct(public $n) {} function __destruct() { echo "destruct {$this->n}\n"; } function name() { return $this->n; } }
function make($n) { return new T($n); }
register_shutdown_function(function () {
    echo "shutdown 1 starts\n";
    echo "name: ", (new T('temporary'))->name(), "\n";
    make('discarded');
    echo "after discarded\n";
    $local = new T('local');
    $local = null;
    echo "after local\n";
    strlen(make('argument')->name());
    echo "shutdown 1 ends\n";
});
register_shutdown_function(function () {
    echo "shutdown 2\n";
    register_shutdown_function(function () { new T('nested'); echo "nested shutdown\n"; });
});
$global = new T('global');
echo "end of script\n";
--EXPECT--
end of script
shutdown 1 starts
name: destruct temporary
temporary
destruct discarded
after discarded
destruct local
after local
destruct argument
shutdown 1 ends
shutdown 2
destruct nested
nested shutdown
destruct global
