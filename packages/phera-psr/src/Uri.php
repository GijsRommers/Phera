<?php

declare(strict_types=1);

namespace Phera\Psr;

use Psr\Http\Message\UriInterface;

/** A PSR-7 URI (RFC 3986). */
final class Uri implements UriInterface
{
    private const DEFAULT_PORTS = ['http' => 80, 'https' => 443, 'ws' => 80, 'wss' => 443];

    /** Bytes a path or query may hold as they are; others are percent-encoded. */
    private const UNRESERVED = 'a-zA-Z0-9_\-.~!$&\'()*+,;=%:@\/';

    private string $scheme = '';
    private string $userInfo = '';
    private string $host = '';
    private ?int $port = null;
    private string $path = '';
    private string $query = '';
    private string $fragment = '';

    public function __construct(string $uri = '')
    {
        if ($uri === '') {
            return;
        }
        $parts = parse_url($uri);
        if ($parts === false) {
            throw new \InvalidArgumentException("\"$uri\" is not a valid URI");
        }
        $this->scheme = strtolower($parts['scheme'] ?? '');
        $this->userInfo = isset($parts['user'])
            ? self::encodeUserInfo($parts['user']) . (isset($parts['pass']) ? ':' . self::encodeUserInfo($parts['pass']) : '')
            : '';
        $this->host = strtolower($parts['host'] ?? '');
        $this->port = self::normalizePort($this->scheme, $parts['port'] ?? null);
        $this->path = self::encode($parts['path'] ?? '');
        $this->query = self::encode($parts['query'] ?? '');
        $this->fragment = self::encode($parts['fragment'] ?? '');
    }

    public function getScheme(): string
    {
        return $this->scheme;
    }

    public function getAuthority(): string
    {
        if ($this->host === '') {
            return '';
        }
        $authority = $this->userInfo === '' ? $this->host : "{$this->userInfo}@{$this->host}";
        return $this->port === null ? $authority : "$authority:{$this->port}";
    }

    public function getUserInfo(): string
    {
        return $this->userInfo;
    }

    public function getHost(): string
    {
        return $this->host;
    }

    public function getPort(): ?int
    {
        return $this->port;
    }

    public function getPath(): string
    {
        return $this->path;
    }

    public function getQuery(): string
    {
        return $this->query;
    }

    public function getFragment(): string
    {
        return $this->fragment;
    }

    public function withScheme(string $scheme): UriInterface
    {
        $copy = clone $this;
        $copy->scheme = strtolower($scheme);
        $copy->port = self::normalizePort($copy->scheme, $this->port);
        return $copy;
    }

    public function withUserInfo(string $user, ?string $password = null): UriInterface
    {
        $copy = clone $this;
        $copy->userInfo = $user === ''
            ? ''
            : self::encodeUserInfo($user) . ($password !== null && $password !== '' ? ':' . self::encodeUserInfo($password) : '');
        return $copy;
    }

    public function withHost(string $host): UriInterface
    {
        $copy = clone $this;
        $copy->host = strtolower($host);
        return $copy;
    }

    public function withPort(?int $port): UriInterface
    {
        $copy = clone $this;
        $copy->port = self::normalizePort($this->scheme, $port);
        return $copy;
    }

    public function withPath(string $path): UriInterface
    {
        $copy = clone $this;
        $copy->path = self::encode($path);
        return $copy;
    }

    public function withQuery(string $query): UriInterface
    {
        $copy = clone $this;
        $copy->query = self::encode(ltrim($query, '?'));
        return $copy;
    }

    public function withFragment(string $fragment): UriInterface
    {
        $copy = clone $this;
        $copy->fragment = self::encode(ltrim($fragment, '#'));
        return $copy;
    }

    public function __toString(): string
    {
        $uri = $this->scheme === '' ? '' : "{$this->scheme}:";
        $authority = $this->getAuthority();
        $path = $this->path;
        if ($authority !== '' || $this->scheme === 'file') {
            $uri .= "//$authority";
            if ($path !== '' && $path[0] !== '/') {
                $path = "/$path";
            }
        } elseif (str_starts_with($path, '//')) {
            $path = '/' . ltrim($path, '/');
        }
        $uri .= $path;
        if ($this->query !== '') {
            $uri .= "?{$this->query}";
        }
        if ($this->fragment !== '') {
            $uri .= "#{$this->fragment}";
        }
        return $uri;
    }

    private static function normalizePort(string $scheme, ?int $port): ?int
    {
        if ($port === null) {
            return null;
        }
        if ($port < 0 || $port > 65535) {
            throw new \InvalidArgumentException("Port $port is not between 0 and 65535");
        }
        return (self::DEFAULT_PORTS[$scheme] ?? null) === $port ? null : $port;
    }

    /** Percent-encodes what may not appear as it is; `%XX` already there stays. */
    private static function encode(string $part): string
    {
        return preg_replace_callback(
            '/(?:[^' . self::UNRESERVED . '?]++|%(?![A-Fa-f0-9]{2}))/',
            static fn (array $match): string => rawurlencode($match[0]),
            $part,
        );
    }

    private static function encodeUserInfo(string $part): string
    {
        return preg_replace_callback(
            '/(?:[^a-zA-Z0-9_\-.~!$&\'()*+,;=%]++|%(?![A-Fa-f0-9]{2}))/',
            static fn (array $match): string => rawurlencode($match[0]),
            $part,
        );
    }
}
