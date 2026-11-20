--TEST--
Formatting errors keep the native function frame
--FILE--
<?php
function showTrace(Closure $call): void {
    try {
        $call();
    } catch (Throwable $e) {
        echo get_class($e), ': ', $e->getMessage(), ' [', $e->getTrace()[0]['function'], "]\n";
    }
}
showTrace(fn() => sprintf('%s'));
showTrace(fn() => printf('%s %s', 'a'));
showTrace(fn() => vsprintf('%s %s', ['a']));
showTrace(fn() => vprintf('%s %s', ['a']));
$format = '%q';
showTrace(fn() => sprintf($format, 'a'));
?>
--EXPECT--
ArgumentCountError: 2 arguments are required, 1 given [sprintf]
ArgumentCountError: 3 arguments are required, 2 given [printf]
ValueError: The arguments array must contain 2 items, 1 given [vsprintf]
ValueError: The arguments array must contain 2 items, 1 given [vprintf]
ValueError: Unknown format specifier "q" [sprintf]
