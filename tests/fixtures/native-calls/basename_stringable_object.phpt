--TEST--
basename() takes its arguments like any built-in function: objects that convert to a string, names, errors
--FILE--
<?php
class Name { function __construct(private string $text) {} function __toString(): string { return $this->text; } }
echo basename(new SplFileInfo('/a/b.txt')), "\n";
echo basename(new Name('/srv/app/Kernel.php'), new Name('.php')), "\n";
echo basename(path: '/a/b.tar.gz', suffix: '.gz'), "|", basename(suffix: '.txt', path: 'c.txt'), "\n";
var_dump(basename(''), basename('/'), basename('a//'), basename('.txt', '.txt'), basename('/a/b', 'b'), basename('x.txt', ''), basename(12.5), basename(true));
var_dump(array_map('basename', ['/a/b', 'c/d/']), array_map(basename(...), ['/e/f']), call_user_func('basename', '/g/h.php', '.php'));
foreach ([[[]], [new stdClass], ['a', []]] as $arguments) {
    try { basename(...$arguments); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
try { basename(); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
try { basename('a', 'b', 'c'); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
set_error_handler(function ($level, $message) { echo "$message\n"; return true; });
echo basename(null), "|\n";
var_dump((new ReflectionFunction('basename'))->getNumberOfParameters(), function_exists('basename'));
--EXPECT--
b.txt
Kernel
b.tar|c
string(0) ""
string(0) ""
string(1) "a"
string(4) ".txt"
string(1) "b"
string(5) "x.txt"
string(4) "12.5"
string(1) "1"
array(2) {
  [0]=>
  string(1) "b"
  [1]=>
  string(1) "d"
}
array(1) {
  [0]=>
  string(1) "f"
}
string(1) "h"
basename(): Argument #1 ($path) must be of type string, array given
basename(): Argument #1 ($path) must be of type string, stdClass given
basename(): Argument #2 ($suffix) must be of type string, array given
basename() expects at least 1 argument, 0 given
basename() expects at most 2 arguments, 3 given
basename(): Passing null to parameter #1 ($path) of type string is deprecated
|
int(2)
bool(true)
