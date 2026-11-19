--TEST--
Namespace fallback, named and spread calls retain the native error frame
--FILE--
<?php
declare(strict_types=1);
namespace NativeCallsTrace;
foreach ([fn () => strlen(1), fn () => \strlen(1), fn () => \strlen(string: 1), fn () => \strlen(...[1])] as $callback) {
    try {
        $callback();
    } catch (\TypeError $error) {
        echo $error->getTrace()[0]['function'] === 'strlen' ? "strlen\n" : "no native frame\n";
    }
}
--EXPECT--
strlen
no native frame
strlen
strlen
