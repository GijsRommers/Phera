--TEST--
clamp preserves the chosen value and validates its bounds
--FILE--
<?php
var_dump(function_exists('clamp'));
foreach ([
    fn() => clamp(3, 0, 5),
    fn() => clamp(-1, 0, 5),
    fn() => clamp("9", 0, 5),
    fn() => clamp("3", 0, 5),
    fn() => clamp(NAN, 0, 5),
    fn() => clamp(3, NAN, 5),
    fn() => clamp(3, 0, NAN),
    fn() => clamp(3, 5, 0),
    fn() => clamp(max: 4, min: 2, value: 9),
    fn() => clamp(['a' => 1], [], ['a' => 2]),
] as $call) {
    try { var_dump($call()); }
    catch (Throwable $error) { echo $error::class, ': ', $error->getMessage(), PHP_EOL; }
}
--EXPECT--
bool(false)
Error: Call to undefined function clamp()
Error: Call to undefined function clamp()
Error: Call to undefined function clamp()
Error: Call to undefined function clamp()
Error: Call to undefined function clamp()
Error: Call to undefined function clamp()
Error: Call to undefined function clamp()
Error: Call to undefined function clamp()
Error: Call to undefined function clamp()
Error: Call to undefined function clamp()
