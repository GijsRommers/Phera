<?php
declare(strict_types=1);

$callback = static fn (): int => 'fixture';
class ReturnSample
{
    public function read(): int { return 'fixture'; }
}
