<?php
final class BranchValue
{
    private static ?self $cached = null;

    public function __construct(public ?self $child) {}

    public static function cached(): self
    {
        if (self::$cached === null) {
            self::$cached = new self(null);
        }

        return self::$cached;
    }
}

function chooseValue(?BranchValue $candidate): BranchValue {
    return $candidate ?: new BranchValue(null);
}

function chooseChild(BranchValue $candidate): BranchValue {
    if ($candidate->child) {
        return $candidate->child;
    }

    return new BranchValue(null);
}

function useCallbacks(string $text): string
{
    set_error_handler(static fn ($severity, $message) => strlen($message) > $severity);

    return preg_replace_callback('/x/', static fn ($parts) => substr($parts[0], 1), $text) ?? '';
}
