<?php
declare(strict_types=1);

function acceptCapture(string $text): void {}

function readCaptures(string $subject): void
{
    preg_match_all('/(a)(?:(b))*/', $subject, $matches, PREG_SET_ORDER | PREG_UNMATCHED_AS_NULL);
    foreach ($matches as $match) {
        acceptCapture($match[1]);
        acceptCapture($match[2]);
    }
}
