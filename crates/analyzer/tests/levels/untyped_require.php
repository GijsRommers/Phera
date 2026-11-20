<?php

function readData(): string
{
    $data = require __DIR__ . '/required_data.php';
    return $data['name'];
}
