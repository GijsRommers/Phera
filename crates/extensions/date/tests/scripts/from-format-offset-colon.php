<?php
try {
    $date = DateTimeImmutable::createFromFormat("!Y-m-d H:iP", "2024-02-29 03:04+05:45");
    if ($date !== false) {
        echo $date->format('Y-m-d\TH:i:s.uP e I'), "\n";
    } else {
        echo "false\n";
    }
    echo json_encode(DateTimeImmutable::getLastErrors()), "\n";
} catch (Throwable $error) {
    echo get_class($error), " ", $error->getMessage(), "\n";
}
