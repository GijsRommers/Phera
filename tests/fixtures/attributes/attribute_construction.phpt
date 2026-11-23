--TEST--
Attribute targets, flags and constructors use declaration semantics
--FILE--
<?php
declare(strict_types=1);
#[Attribute('1')] class NonIntegerFlags {}
#[Attribute(256)] class InvalidFlags {}
#[Attribute(unknown: Attribute::TARGET_CLASS, ignored: 256)] class ExtraFlags {}
#[Attribute(unknown: Attribute::TARGET_PROPERTY, flags: Attribute::TARGET_CLASS)] class WrongTarget {}
#[Attribute] class TypedTag { public function __construct(int $number) {} }
#[Attribute] class PrivateTag { private function __construct() {} }

#[NonIntegerFlags] class First {}
#[InvalidFlags] class Second {}
#[ExtraFlags(123)] class Third {}
#[WrongTarget] class Fourth {}
#[TypedTag('9')] class Fifth {}
#[PrivateTag] class Sixth {}

foreach ([First::class, Second::class, Third::class, Fourth::class, Fifth::class, Sixth::class] as $name) {
    try {
        $attribute = (new ReflectionClass($name))->getAttributes()[0]->newInstance();
        echo $attribute::class, "\n";
    } catch (Throwable $exception) {
        echo $exception::class, ': ', $exception->getMessage(), "\n";
    }
}
--EXPECTF--
Error: Attribute::__construct(): Argument #1 ($flags) must be of type int, string given
Error: Invalid attribute flags specified
ExtraFlags
Error: Attribute "WrongTarget" cannot target class (allowed targets: property)
TypeError: TypedTag::__construct(): Argument #1 ($number) must be of type int, string given, called in %s on line 14
Error: Call to private PrivateTag::__construct() from global scope
