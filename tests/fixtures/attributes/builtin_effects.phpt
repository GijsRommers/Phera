--TEST--
Built-in attributes affect ordinary calls, reads and exception traces
--FILE--
<?php
class ParentClass { public function action() {} }
class ChildClass extends ParentClass {
    #[Override]
    public function action() {}

    #[Deprecated(message: 'Use VERSION', since: '1.5')]
    public const OLD = 4;
}

#[Deprecated('Use revised()', since: '2.0')]
function old(): int { return 1; }

#[NoDiscard('Use the checksum')]
function checksum(): int { echo "checksum\n"; return 7; }

function protect(#[SensitiveParameter] string $secret): void {
    throw new Exception('denied');
}

echo old(), "\n";
echo ChildClass::OLD, "\n";
checksum();
echo checksum(), "\n";
(new ChildClass())->action();
try {
    protect('password');
} catch (Exception $e) {
    $value = $e->getTrace()[0]['args'][0];
    echo $value::class, ": ", $value->getValue(), "\n";
}
--EXPECTF--
Deprecated: Function old() is deprecated since 2.0, Use revised() in %s on line 21
1

Deprecated: Constant ChildClass::OLD is deprecated since 1.5, Use VERSION in %s on line 22
4

Warning: The return value of function checksum() should either be used or intentionally ignored by casting it as (void), Use the checksum in %s on line 23
checksum
checksum
7
SensitiveParameterValue: password
