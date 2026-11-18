<?php
set_error_handler(function ($level, $message) {
    echo $level, ' ', $message, "\n";
    return true;
});
function inspect_interval_properties($interval, $properties) {
    foreach ($properties as $name) {
        $value = $interval->$name;
        echo json_encode([
            $name, $value, gettype($value), isset($interval->$name),
            property_exists($interval, $name), property_exists(DateInterval::class, $name),
        ]), "\n";
    }
}
$names = ['y', 'm', 'd', 'h', 'i', 's', 'f', 'invert', 'days', 'from_string', 'date_string'];
$date = new DateTimeImmutable('2024-01-01 08:00:00 UTC');
foreach ([new DateInterval('P3D'), DateInterval::createFromDateString('3 days ago + 2 hours'),
    $date->diff($date->modify('+4 days'))] as $interval) {
    inspect_interval_properties($interval, $names);
    echo json_encode($interval->__serialize()), "\n";
    foreach ($names as $name) {
        $interval->$name = '5.4321987';
    }
    inspect_interval_properties($interval, $names);
    echo json_encode($interval->__serialize()), "\n";
    echo $interval->format('%R %y %m %d %h %i %s %F %a'), "\n";
    $reference =& $interval->d;
    $reference = 100;
    unset($reference);
    echo $interval->d, "\n";
    try {
        $interval->d =& $reference;
    } catch (Error $error) {
        echo $error->getMessage(), "\n";
    }
    unset($interval->d);
    echo $interval->d, "\n";
}
class UninitializedInterval extends DateInterval { public function __construct() {} }
inspect_interval_properties(new UninitializedInterval(), $names);
class ShadowedInterval extends DateInterval { private $d = 100; }
$shadow = new ShadowedInterval('P3D');
echo $shadow->d, "\n";
$shadow->d = 7;
echo $shadow->d, "\n";
