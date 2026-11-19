--TEST--
Suspension preserves a native callback and shared request configuration
--FILE--
<?php
$fiber = new Fiber(function () {
    $fiber = Fiber::getCurrent();
    return array_map(function ($number) use ($fiber) {
        var_dump(Fiber::getCurrent() === $fiber);
        $value = Fiber::suspend($number);
        return [ini_get('default_charset'), $value * 2];
    }, [1, 2]);
});
var_dump($fiber->start());
ini_set('default_charset', 'ASCII');
var_dump($fiber->resume(3));
ini_restore('default_charset');
var_dump($fiber->resume(4));
var_dump($fiber->getReturn());
--EXPECT--
bool(true)
int(1)
bool(true)
int(2)
NULL
array(2) {
  [0]=>
  array(2) {
    [0]=>
    string(5) "ASCII"
    [1]=>
    int(6)
  }
  [1]=>
  array(2) {
    [0]=>
    string(5) "UTF-8"
    [1]=>
    int(8)
  }
}
