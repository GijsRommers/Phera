<?php
$a = new DateTime('2024-02-08 10:11:12.654321 Australia/Adelaide');
$b = DateTimeImmutable::createFromMutable($a);
$c = DateTimeImmutable::createFromInterface($a);
$d = DateTime::createFromImmutable($b);
$e = DateTime::createFromInterface($b);
foreach ([$a, $b, $c, $d, $e] as $value) {
    echo get_class($value), ' ', $value instanceof DateTimeInterface ? 'yes' : 'no', "\n";
    echo $value->format(DateTimeInterface::ATOM), "\n";
    echo $value->format(DateTimeImmutable::RFC3339_EXTENDED), "\n";
}
echo json_encode([$a === $d, $b === $c, $a === $a->modify('+1 day')]), "\n";
