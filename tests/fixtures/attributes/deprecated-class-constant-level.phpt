--TEST--
Deprecated enum cases and class constants emit E_USER_DEPRECATED
--FILE--
<?php
enum Status {
    #[Deprecated('Use New', '2')]
    case Old;
    case New;
}
class Version {
    #[Deprecated]
    const OLD = 1;
}
set_error_handler(static function (int $level, string $message): bool {
    echo $level, ': ', $message, "\n";
    return true;
}, E_USER_DEPRECATED);
echo Status::Old->name, "\n";
echo Version::OLD, "\n";
--EXPECT--
16384: Enum case Status::Old is deprecated since 2, Use New
Old
16384: Constant Version::OLD is deprecated
1
