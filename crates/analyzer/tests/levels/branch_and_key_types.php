<?php

declare(strict_types=1);

function firstText(?string $left, ?string $right): string
{
    if ($left === null && $right === null) {
        throw new InvalidArgumentException();
    }
    if ($left === null) {
        return $right;
    }
    return $left;
}

function writeLabel(string $label, string $value): void
{
    echo $label, $value;
}

/** @param string[][] $rows */
function writeRows(array $rows): void
{
    foreach ($rows as $row) {
        foreach ($row as $label => $value) {
            writeLabel($label, $value);
        }
    }
}

final class MessageBody
{
    /** @var string */
    public $text;

    public function __construct(string $text)
    {
        $this->text = $text;
    }
}

function bodyLength(MessageBody $body): int
{
    return strlen($body->text);
}
