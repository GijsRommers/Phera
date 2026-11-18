--TEST--
A native subclass can override its debug view
--FILE--
<?php
class DebugDate extends DateTime {
    public function __debugInfo(): array {
        echo "debug hook\n";
        return ['display' => $this->format('Y-m-d')];
    }
}
$date = new DebugDate('2000-01-01', new DateTimeZone('UTC'));
print_r([$date]);
--EXPECT--
debug hook
Array
(
    [0] => DebugDate Object
        (
            [display] => 2000-01-01
        )

)
