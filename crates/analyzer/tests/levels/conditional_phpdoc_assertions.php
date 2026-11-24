<?php
declare(strict_types=1);

class AssertedResource
{
    public function size(): int { return 1; }
}

class ResourceChecks
{
    /** @phpstan-assert-if-true AssertedResource $resource */
    public static function available(?AssertedResource $resource): bool
    {
        return $resource !== null;
    }
}

/** @psalm-assert-if-false !null $resource */
function isNullResource(?AssertedResource $resource): bool
{
    return $resource === null;
}

function checkedSize(?AssertedResource $resource): int
{
    return ResourceChecks::available($resource) ? $resource->size() : 0;
}

function checkedInvertedSize(?AssertedResource $resource): int
{
    return !isNullResource($resource) ? $resource->size() : 0;
}
