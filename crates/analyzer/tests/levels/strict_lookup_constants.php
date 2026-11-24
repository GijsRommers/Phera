<?php
declare(strict_types=1);

interface Mode
{
    const READ = 1;
    const WRITE = 2;

    public function current(): string|int|float|bool;
}
function useMode(int $mode): void {}

function allowedMode(Mode $mode): void
{
    if (in_array($mode->current(), [Mode::READ, Mode::WRITE], true)) {
        useMode($mode->current());
    }
}
