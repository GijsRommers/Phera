<?php
declare(strict_types=1);

final class Payload
{
    public function name(): string
    {
        return 'fixture';
    }
}

final class RequireValue
{
    /**
     * @template T
     * @param T|null $value
     * @return T
     */
    public static function present(mixed $value): mixed
    {
        return $value ?? throw new LogicException();
    }
}

function callNullableTemplate(?Payload $payload): string
{
    return RequireValue::present($payload)->name();
}
