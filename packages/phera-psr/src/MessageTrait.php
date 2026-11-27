<?php

declare(strict_types=1);

namespace Phera\Psr;

use Psr\Http\Message\StreamInterface;

/**
 * Headers, protocol version and body of a PSR-7 message.
 *
 * Header names keep the case they were set with; lookups ignore case.
 *
 * @internal
 */
trait MessageTrait
{
    /** @var array<string, list<string>> name as given => lines */
    private array $headers = [];

    /** @var array<string, string> lower-case name => name as given */
    private array $headerNames = [];

    private string $protocol = '1.1';

    private ?StreamInterface $stream = null;

    public function getProtocolVersion(): string
    {
        return $this->protocol;
    }

    public function withProtocolVersion(string $version): static
    {
        if ($version === $this->protocol) {
            return $this;
        }
        $copy = clone $this;
        $copy->protocol = $version;
        return $copy;
    }

    public function getHeaders(): array
    {
        return $this->headers;
    }

    public function hasHeader(string $name): bool
    {
        return isset($this->headerNames[strtolower($name)]);
    }

    public function getHeader(string $name): array
    {
        $name = $this->headerNames[strtolower($name)] ?? null;
        return $name === null ? [] : $this->headers[$name];
    }

    public function getHeaderLine(string $name): string
    {
        return implode(', ', $this->getHeader($name));
    }

    public function withHeader(string $name, $value): static
    {
        $lines = self::headerLines($name, $value);
        $copy = clone $this;
        $copy->removeHeader($name);
        $copy->headerNames[strtolower($name)] = $name;
        $copy->headers[$name] = $lines;
        return $copy;
    }

    public function withAddedHeader(string $name, $value): static
    {
        $lines = self::headerLines($name, $value);
        $copy = clone $this;
        $existing = $copy->headerNames[strtolower($name)] ?? null;
        if ($existing === null) {
            $copy->headerNames[strtolower($name)] = $name;
            $copy->headers[$name] = $lines;
        } else {
            $copy->headers[$existing] = [...$copy->headers[$existing], ...$lines];
        }
        return $copy;
    }

    public function withoutHeader(string $name): static
    {
        if (!$this->hasHeader($name)) {
            return $this;
        }
        $copy = clone $this;
        $copy->removeHeader($name);
        return $copy;
    }

    public function getBody(): StreamInterface
    {
        return $this->stream ??= Stream::create('');
    }

    public function withBody(StreamInterface $body): static
    {
        if ($body === $this->stream) {
            return $this;
        }
        $copy = clone $this;
        $copy->stream = $body;
        return $copy;
    }

    /** @param array<string, string|list<string>> $headers */
    private function setHeaders(array $headers): void
    {
        foreach ($headers as $name => $value) {
            $name = (string) $name;
            $lines = self::headerLines($name, $value);
            $existing = $this->headerNames[strtolower($name)] ?? null;
            if ($existing === null) {
                $this->headerNames[strtolower($name)] = $name;
                $this->headers[$name] = $lines;
            } else {
                $this->headers[$existing] = [...$this->headers[$existing], ...$lines];
            }
        }
    }

    private function removeHeader(string $name): void
    {
        $lower = strtolower($name);
        if (isset($this->headerNames[$lower])) {
            unset($this->headers[$this->headerNames[$lower]], $this->headerNames[$lower]);
        }
    }

    /** @return list<string> */
    private static function headerLines(string $name, mixed $value): array
    {
        if (preg_match('/^[!#$%&\'*+\-.^_`|~0-9A-Za-z]+$/D', $name) !== 1) {
            throw new \InvalidArgumentException("\"$name\" is not a valid header name");
        }
        $values = is_array($value) ? array_values($value) : [$value];
        if ($values === []) {
            throw new \InvalidArgumentException("Header \"$name\" needs at least one value");
        }
        $lines = [];
        foreach ($values as $line) {
            if (!is_string($line) && !is_int($line) && !is_float($line)) {
                throw new \InvalidArgumentException("Header \"$name\" values must be strings");
            }
            $line = trim((string) $line, " \t");
            if (preg_match('/[\r\n\0]/', $line) === 1) {
                throw new \InvalidArgumentException("Header \"$name\" must not contain CR, LF or NUL");
            }
            $lines[] = $line;
        }
        return $lines;
    }
}
