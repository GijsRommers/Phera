<?php
declare(strict_types=1);

namespace Doctrine\ORM {
    abstract class AbstractQuery
    {
        abstract public function getSingleScalarResult(): mixed;
    }
}

namespace Fixture {
    use Doctrine\ORM\AbstractQuery;

    /**
     * @template T
     * @param \Closure(): T $callback
     * @return T
     */
    function memoized(\Closure $callback): mixed
    {
        return $callback();
    }

    /** @param AbstractQuery<null, int> $query */
    function countRow(AbstractQuery $query): int
    {
        return (int) memoized(static fn (): mixed => $query->getSingleScalarResult());
    }
}
