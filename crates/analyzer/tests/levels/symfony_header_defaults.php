<?php
declare(strict_types=1);

namespace Symfony\Component\HttpFoundation {
    class HeaderBag
    {
        public function get(string $key, ?string $default = null): ?string
        {
            return $default;
        }
    }
}

namespace Fixture {
    use Symfony\Component\HttpFoundation\HeaderBag;

    function acceptHeader(HeaderBag $headers): string
    {
        return $headers->get('Accept', 'text/plain');
    }
}
