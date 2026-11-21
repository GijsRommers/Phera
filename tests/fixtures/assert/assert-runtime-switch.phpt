--TEST--
Direct assertion guards follow runtime INI changes and resolved function names
--INI--
zend.assertions=0
--FILE--
<?php
namespace Parts {
    function probe(): bool {
        echo "ordinary function\n";
        return false;
    }
}

namespace App {
    use function Parts\probe as assert;
    use function \assert as check;

    $n = 0;
    var_dump(\assert(++$n), check(++$n), $n);
    var_dump(assert());
    var_dump(ini_set('zend.assertions', '1'));

    try { check($n > 0); }
    catch (\AssertionError $e) { echo $e->getMessage(), "\n"; }

    try { \assert(false, new \LogicException('direct')); }
    catch (\LogicException $e) { echo $e->getMessage(), "\n"; }

    try { \assert(true, new \stdClass); }
    catch (\TypeError $e) { echo $e->getMessage(), "\n"; }

    try { \assert(false, new class { function __toString() { return 'converted'; } }); }
    catch (\AssertionError $e) { echo $e->getMessage(), "\n"; }

    set_error_handler(function ($level, $message) { echo $message, "\n"; });
    var_dump(ini_set('zend.assertions', '-1'));
    ini_restore('zend.assertions');
    var_dump(\assert(++$n), $n);
}
?>
--EXPECT--
bool(true)
bool(true)
int(0)
ordinary function
bool(false)
string(1) "0"
assert($n > 0)
direct
assert(): Argument #2 ($description) must be of type Throwable|string|null, stdClass given
converted
zend.assertions may be completely enabled or disabled only in php.ini
bool(false)
bool(true)
int(0)
