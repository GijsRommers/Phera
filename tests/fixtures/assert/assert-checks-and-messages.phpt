--TEST--
assert() checks its condition and reports it as written
--FILE--
<?php
namespace {
function check(string $label, callable $f) { try { var_dump($f()); } catch (Throwable $e) { echo $label, ': ', get_class($e), ': ', $e->getMessage(), "\n"; } }
$a = 5; $o = new stdClass; $list = [1, 2];
check('true', fn() => assert(true));
check('expr ok', fn() => assert($a > 1));
check('expr fails', fn() => assert($a > 10));
check('false', fn() => assert(false));
check('instanceof', fn() => assert($o instanceof Countable));
check('call', fn() => assert(is_string($a) && strlen($a) > 0));
check('array', fn() => assert($list[0] === 2));
check('message', fn() => assert($a === 6, 'five is not six'));
check('throwable', fn() => assert($a === 6, new DomainException('custom exception')));
check('null description', fn() => assert(false, null));
check('zero', fn() => assert(0));
check('empty string', fn() => assert(''));
check('string', fn() => assert('non-empty'));
check('upper case', fn() => ASSERT($a < 0));
check('qualified', fn() => \assert($a < 0));
check('callback', fn() => call_user_func('assert', false));
check('callback ok', fn() => call_user_func('assert', 1));
check('callback message', fn() => call_user_func_array('assert', [false, 'from callback']));
check('array_map', fn() => array_map('assert', [true, 1]));
check('named', fn() => assert(assertion: false, description: 'named arguments'));
check('named one', fn() => assert(assertion: false));
check('no arguments', fn() => assert());
var_dump(function_exists('assert'), is_callable('assert'));
function side() { echo "evaluated\n"; return true; }
assert(side());
try { assert($a > 99); } catch (AssertionError $e) { var_dump($e instanceof Error, $e->getLine(), $e->getCode()); }
$f = assert(...); check('first class', fn() => $f(false, 'first class callable'));
}
namespace App {
function run() { try { assert(1 > 2); } catch (\AssertionError $e) { echo 'namespaced: ', $e->getMessage(), "\n"; } }
run();
}
--EXPECT--
bool(true)
bool(true)
expr fails: AssertionError: assert($a > 10)
false: AssertionError: assert(false)
instanceof: AssertionError: assert($o instanceof Countable)
call: AssertionError: assert(is_string($a) && strlen($a) > 0)
array: AssertionError: assert($list[0] === 2)
message: AssertionError: five is not six
throwable: DomainException: custom exception
null description: AssertionError: 
zero: AssertionError: assert(0)
empty string: AssertionError: assert('')
bool(true)
upper case: AssertionError: assert($a < 0)
qualified: AssertionError: assert($a < 0)
callback: AssertionError: 
bool(true)
callback message: AssertionError: from callback
array(2) {
  [0]=>
  bool(true)
  [1]=>
  bool(true)
}
named: AssertionError: named arguments
named one: AssertionError: assert(assertion: false)
no arguments: ArgumentCountError: assert() expects at least 1 argument, 0 given
bool(true)
bool(true)
evaluated
bool(true)
int(30)
int(1)
first class: AssertionError: first class callable
namespaced: assert(1 > 2)
