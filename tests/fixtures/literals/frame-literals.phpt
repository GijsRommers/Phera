--TEST--
Literal tables follow calls across files and preserve array copy on write
--FILE--
<?php
require __DIR__ . '/frame-literals.inc';

function homeLiteral(): array {
    echo "home\n";
    return ['home', 2.5];
}

function interleaveLiterals(): void {
    echo "before\n";
    $first = libraryLiteral();
    $first[0] = 'changed';
    echo $first[0], ' ', $first[1], "\n";
    echo homeLiteral()[0], "\n";
    echo libraryLiteral()[0], "\n";
    echo "after\n";
}

class Guard {
    public function __destruct() { echo "released\n"; }
}

interleaveLiterals();
$guard = new Guard();
interleaveLiterals();
unset($guard);
--EXPECT--
before
library
changed 4.25
home
home
library
library
after
before
library
changed 4.25
home
home
library
library
after
released
