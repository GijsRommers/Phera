--TEST--
ReflectionGenerator traces the current delegate with PHP backtrace options and live arguments
--FILE--
<?php
function leaf(#[SensitiveParameter] $secret, $number) {
    $number++;
    yield $number;
}
class Source {
    function outer($secret, $number, ...$extra): Generator {
        yield from leaf($secret, $number);
    }
}
$source = new Source;
$generator = $source->outer('private', 5, 6, tail: 7);
$reflection = new ReflectionGenerator($generator);
var_dump($reflection->getExecutingGenerator() === $generator);
$generator->current();
echo $reflection->getExecutingLine(), "
";
var_dump($reflection->getThis() === $source, $reflection->getExecutingGenerator() !== $generator);
echo (new ReflectionGenerator($reflection->getExecutingGenerator()))->getFunction()->getName(), "
";
foreach ([DEBUG_BACKTRACE_PROVIDE_OBJECT, 0, DEBUG_BACKTRACE_IGNORE_ARGS] as $option) {
    $trace = $reflection->getTrace($option);
    foreach ($trace as &$frame) {
        if (isset($frame['file'])) $frame['file'] = basename($frame['file']);
        if (isset($frame['object'])) $frame['object'] = get_class($frame['object']);
        if (($frame['args'][0] ?? null) instanceof SensitiveParameterValue) {
            $frame['args'][0] = 'redacted';
        }
    }
    echo json_encode($trace), "
";
}
--EXPECT--
bool(true)
8
bool(true)
bool(true)
leaf
[{"file":"reflection-delegating-trace.php","line":8,"function":"leaf","args":["redacted",6]},{"function":"outer","class":"Source","object":"Source","type":"->","args":{"0":"private","1":5,"2":6,"tail":7}}]
[{"file":"reflection-delegating-trace.php","line":8,"function":"leaf","args":["redacted",6]},{"function":"outer","class":"Source","type":"->","args":{"0":"private","1":5,"2":6,"tail":7}}]
[{"file":"reflection-delegating-trace.php","line":8,"function":"leaf"},{"function":"outer","class":"Source","type":"->"}]
