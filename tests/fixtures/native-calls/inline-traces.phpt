--TEST--
Specialized and ordinary calls preserve exact error lines and frames
--FILE--
<?php
declare(strict_types=1);
function printNativeError(Throwable $error) {
    echo $error->getMessage(), '|', $error->getFile() === __FILE__ ? $error->getLine() : '-', "\n";
    foreach ($error->getTrace() as $frame) {
        echo $frame['class'] ?? '', $frame['type'] ?? '', $frame['function'], ':';
        echo ($frame['file'] ?? null) === __FILE__ ? $frame['line'] : '-', "\n";
    }
}
function builtinLength($value) {
    return strlen($value);
}
function namedLength($value) {
    return strlen(string: $value);
}
function expandedLength($value) {
    return strlen(...[$value]);
}
function builtinCount($value) {
    return count($value);
}
function builtinClass($value) {
    return get_class($value);
}
function builtinKey($key) {
    return array_key_exists($key, []);
}
function builtinFormat($value) {
    return sprintf('%s', $value);
}
function builtinCast($value) {
    return strval($value);
}
foreach ([
    ['builtinLength', 42],
    ['namedLength', 42],
    ['expandedLength', 42],
    ['builtinCount', 42],
    ['builtinClass', 42],
    ['builtinKey', new stdClass],
    ['builtinFormat', new stdClass],
    ['builtinCast', new stdClass],
] as [$function, $value]) {
    try {
        $function($value);
    } catch (Throwable $error) {
        printNativeError($error);
    }
}
try {
    get_called_class();
} catch (Throwable $error) {
    printNativeError($error);
}
--EXPECT--
strlen(): Argument #1 ($string) must be of type string, int given|11
builtinLength:45
strlen(): Argument #1 ($string) must be of type string, int given|14
strlen:14
namedLength:45
strlen(): Argument #1 ($string) must be of type string, int given|17
strlen:17
expandedLength:45
count(): Argument #1 ($value) must be of type Countable|array, int given|20
builtinCount:45
get_class(): Argument #1 ($object) must be of type object, int given|23
builtinClass:45
Cannot access offset of type stdClass on array|26
builtinKey:45
Object of class stdClass could not be converted to string|29
builtinFormat:45
Object of class stdClass could not be converted to string|32
builtinCast:45
get_called_class() must be called from within a class|51
