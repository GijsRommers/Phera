<?php
enum Mode { case Read; case Write; }
final readonly class Job
{
    public function __construct(public Mode $mode) {}
}
function useGuard(bool $enabled, string $input, Job $job): void
{
    $matched = $enabled && preg_match('/a/', $input, $matches) === 1;
    preg_match('/b/', $input, $other);
    if ($matched) {
        echo $matches[0];
    }
    if ($job->mode === Mode::Read) {
        echo $input;
    } else {
        $result = $input;
    }
    if ($job->mode === Mode::Write) {
        echo $result;
    }
}
