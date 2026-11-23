--TEST--
A failing built-in in a namespace is a call of the function, with its frame in the trace; the qualified form is PHP's operator, with none (weak types).
--FILE--
<?php
// A failing built-in in a namespace is a call of the function, with its frame
// in the trace; the qualified form is PHP's operator, with none (weak types).
namespace App;
final class Box {}
$cases = [
    'strlen([])' => fn () => strlen([]), '\\strlen([])' => fn () => \strlen([]),
    'strlen(1)' => fn () => strlen(1), '\\strlen(1)' => fn () => \strlen(1),
    'count(5)' => fn () => count(5), '\\count(5)' => fn () => \count(5),
    'sizeof(5)' => fn () => sizeof(5), 'get_class(5)' => fn () => get_class(5), '\\get_class(5)' => fn () => \get_class(5),
    'strlen(new Box)' => fn () => strlen(new Box),
];
for ($round = 0; $round < 2; $round++) {
    foreach ($cases as $label => $call) {
        try {
            var_dump($call());
        } catch (\TypeError $error) {
            echo $label, ': ', $error->getMessage(), ' [', $error->getTrace()[0]['function'] ?? '-', "]\n";
        }
    }
}
--EXPECTF--
strlen([]): strlen(): Argument #1 ($string) must be of type string, array given [strlen]
\strlen([]): strlen(): Argument #1 ($string) must be of type string, array given [{closure:%s:7}]
int(1)
int(1)
count(5): count(): Argument #1 ($value) must be of type Countable|array, int given [count]
\count(5): count(): Argument #1 ($value) must be of type Countable|array, int given [{closure:%s:9}]
sizeof(5): sizeof(): Argument #1 ($value) must be of type Countable|array, int given [sizeof]
get_class(5): get_class(): Argument #1 ($object) must be of type object, int given [get_class]
\get_class(5): get_class(): Argument #1 ($object) must be of type object, int given [{closure:%s:10}]
strlen(new Box): strlen(): Argument #1 ($string) must be of type string, App\Box given [strlen]
strlen([]): strlen(): Argument #1 ($string) must be of type string, array given [strlen]
\strlen([]): strlen(): Argument #1 ($string) must be of type string, array given [{closure:%s:7}]
int(1)
int(1)
count(5): count(): Argument #1 ($value) must be of type Countable|array, int given [count]
\count(5): count(): Argument #1 ($value) must be of type Countable|array, int given [{closure:%s:9}]
sizeof(5): sizeof(): Argument #1 ($value) must be of type Countable|array, int given [sizeof]
get_class(5): get_class(): Argument #1 ($object) must be of type object, int given [get_class]
\get_class(5): get_class(): Argument #1 ($object) must be of type object, int given [{closure:%s:10}]
strlen(new Box): strlen(): Argument #1 ($string) must be of type string, App\Box given [strlen]
