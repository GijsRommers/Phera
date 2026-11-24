<?php
declare(strict_types=1);

class GuardedLabel
{
    public function text(): string { return 'label'; }
}

function labelWithGuard(?GuardedLabel $label, ?GuardedLabel $newLabel): ?string
{
    $known = $label !== null;
    if ($known) {
        return $label->text();
    }

    return null;
}

function staleGuard(?GuardedLabel $label, ?GuardedLabel $newLabel): ?string
{
    $known = $label !== null;
    $label = $newLabel;
    if ($known) {
        return $label->text();
    }

    return null;
}
