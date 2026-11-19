--TEST--
Native calls can suspend through a PHP string conversion in a Fiber
--FILE--
<?php
class NativeFiberValue {
    public function __toString(): string {
        return Fiber::suspend('waiting');
    }
}
$fiber = new Fiber(function () {
    return array_map(function (NativeFiberValue $value) {
        return call_user_func('strlen', $value);
    }, [new NativeFiberValue]);
});
var_dump($fiber->start());
foreach ((new ReflectionFiber($fiber))->getTrace(DEBUG_BACKTRACE_IGNORE_ARGS) as $frame) {
    $name = $frame['function'];
    echo $frame['class'] ?? '', $frame['type'] ?? '', str_starts_with($name, '{closure') ? '{closure}' : $name, "\n";
}
$fiber->resume("é");
var_dump($fiber->getReturn());
--EXPECT--
string(7) "waiting"
Fiber::suspend
NativeFiberValue->__toString
strlen
{closure}
array_map
{closure}
array(1) {
  [0]=>
  int(2)
}
