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
        echo $class, '::', $name, ' ', $method->getReturnType(), ' / ',
            $method->getTentativeReturnType(), PHP_EOL;
    }
}
--EXPECT--
DirectoryIterator::key  / mixed
DirectoryIterator::current  / mixed
DateTime::createFromInterface DateTime / 
DateTimeImmutable::__set_state  / DateTimeImmutable
DateTimeImmutable::createFromInterface DateTimeImmutable / 
DateTimeImmutable::modify  / DateTimeImmutable
DateTimeImmutable::add  / DateTimeImmutable
DateTimeImmutable::sub  / DateTimeImmutable
DateTimeImmutable::setTimezone  / DateTimeImmutable
DateTimeImmutable::setTime  / DateTimeImmutable
DateTimeImmutable::setDate  / DateTimeImmutable
DateTimeImmutable::setISODate  / DateTimeImmutable
DateTimeImmutable::setTimestamp  / DateTimeImmutable
DateTimeZone::__set_state  / DateTimeZone
DateInterval::__set_state  / DateInterval
DatePeriod::__set_state  / DatePeriod
