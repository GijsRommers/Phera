--TEST--
DirectoryIterator and date methods report their tentative return types
--FILE--
<?php
foreach ([
    DirectoryIterator::class => ['key', 'current'],
    DateTime::class => ['createFromInterface'],
    DateTimeImmutable::class => [
        '__set_state', 'createFromInterface', 'modify', 'add', 'sub',
        'setTimezone', 'setTime', 'setDate', 'setISODate', 'setTimestamp',
    ],
    DateTimeZone::class => ['__set_state'],
    DateInterval::class => ['__set_state'],
    DatePeriod::class => ['__set_state'],
] as $class => $methods) {
    foreach ($methods as $name) {
        $method = new ReflectionMethod($class, $name);
        echo $class, '::', $name, ' ', $method->getReturnType() ?? '-', ' / ',
            $method->getTentativeReturnType() ?? '-', PHP_EOL;
    }
}
--EXPECT--
DirectoryIterator::key - / string|int
DirectoryIterator::current - / SplFileInfo|static|string
DateTime::createFromInterface - / static
DateTimeImmutable::__set_state - / static
DateTimeImmutable::createFromInterface - / static
DateTimeImmutable::modify - / static
DateTimeImmutable::add - / static
DateTimeImmutable::sub - / static
DateTimeImmutable::setTimezone - / static
DateTimeImmutable::setTime - / static
DateTimeImmutable::setDate - / static
DateTimeImmutable::setISODate - / static
DateTimeImmutable::setTimestamp - / static
DateTimeZone::__set_state - / static
DateInterval::__set_state - / static
DatePeriod::__set_state - / static
