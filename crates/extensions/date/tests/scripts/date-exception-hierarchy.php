<?php
foreach ([
    new DateError('error', 23),
    new DateObjectError('uninitialized', 24),
    new DateRangeError('range', 25),
    new DateException('exception', 26),
    new DateInvalidTimeZoneException('zone', 27),
    new DateInvalidOperationException('operation', 28),
    new DateMalformedStringException('string', 29),
    new DateMalformedIntervalStringException('interval', 30),
    new DateMalformedPeriodStringException('period', 31),
] as $error) {
    echo get_class($error), ' ', $error->getMessage(), ' ', $error->getCode(), "\n";
    echo json_encode([
        $error instanceof DateError,
        $error instanceof Error,
        $error instanceof DateException,
        $error instanceof Exception,
        $error instanceof Throwable,
    ]), "\n";
}
