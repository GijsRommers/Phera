--TEST--
Reflection can read a deprecated enum case without emitting the direct-access diagnostic
--FILE--
<?php
enum Status {
    #[Deprecated('Choose New instead', '2')]
    case Old;
    case New;
}
$case = (new ReflectionEnum(Status::class))->getCase('Old');
echo (int) $case->isDeprecated(), ':', $case->getValue()->name, "\n";
echo Status::Old->name, "\n";
--EXPECTF--
1:Old

Deprecated: Enum case Status::Old is deprecated since 2, Choose New instead in %s on line 9
Old
