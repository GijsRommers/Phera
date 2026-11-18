<?php
try {
    $date = DateTimeImmutable::createFromFormat("!Y-m-d O", "2024-02-29 -0230");
    if ($date !== false) {
        echo $date->format('Y-m-d\TH:i:s.uP e I'), "\n";
    } else {
        echo "false\n";
    }
    echo json_encode(DateTimeImmutable::getLastErrors()), "\n";
} catch (Throwable $error) {
    echo get_class($error), " ", $error->getMessage(), "\n";
}
