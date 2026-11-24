<?php

interface Operation {}

/** @template T of object */
interface TypedOperation extends Operation {}

final class ResultRow
{
    public string $label = '';
}

/** @implements TypedOperation<ResultRow> */
final class ReadOperation implements TypedOperation {}

interface Dispatcher
{
    /**
     * @template T of object
     * @param Operation|TypedOperation<T> $operation
     * @return ($operation is TypedOperation<T> ? T : mixed)
     */
    public function dispatch(Operation $operation): mixed;
}

function dispatchRow(Dispatcher $dispatcher, ReadOperation $operation): string
{
    return $dispatcher->dispatch($operation)->label;
}
