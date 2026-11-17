<?php
try {
    $date = DateTimeImmutable::createFromTimestamp(NAN);
    echo $date->format('Y-m-d\TH:i:s.uP e I'), "\n";
    echo $date->getTimestamp(), ' ', $date->getMicrosecond(), "\n";
} catch (Throwable $error) {
    echo get_class($error), " ", $error->getMessage(), "\n";
}
