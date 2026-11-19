<?php

function integers(int $count): Generator
{
    for ($number = 0; $number < $count; ++$number) {
        yield $number;
    }
}

function delegate(Generator $source): Generator
{
    yield from $source;
}

function receiver(int $count): Generator
{
    $total = 0;
    for ($number = 0; $number < $count; ++$number) {
        $total += yield $number;
    }
    return $total;
}

function sum_iterable(iterable $source): int
{
    $total = 0;
    foreach ($source as $number) {
        $total += $number;
    }
    return $total;
}

function benchmark_generators(int $count): void
{
    echo sum_iterable(integers($count)), "\n";
    echo sum_iterable(delegate(delegate(delegate(integers($count))))), "\n";

    $generator = receiver($count);
    $generator->rewind();
    for ($number = 0; $number < $count; ++$number) {
        $generator->send($number);
    }
    echo $generator->getReturn(), "\n";
}

benchmark_generators(1000000);
