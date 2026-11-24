<?php
declare(strict_types=1);

final class FixtureMode
{
    const OPTIONAL = 1;
    const MULTIPLE = 4;
}

function acceptFlags(?int $flags): void {}

function useFixtureFlags(): void
{
    acceptFlags(FixtureMode::OPTIONAL | FixtureMode::MULTIPLE);
}
