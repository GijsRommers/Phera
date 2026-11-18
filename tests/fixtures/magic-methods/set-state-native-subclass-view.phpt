--TEST--
Export uses the native property view even for a subclass with a debug hook
--FILE--
<?php
class ExportDate extends DateTimeImmutable {
    public function __debugInfo(): array { echo "debug hook\n"; return ['debug' => true]; }
}
$date = new ExportDate('2000-01-01', new DateTimeZone('UTC'));
echo var_export($date, true), "\n";
--EXPECT--
\ExportDate::__set_state(array(
   'date' => '2000-01-01 00:00:00.000000',
   'timezone_type' => 3,
   'timezone' => 'UTC',
))
