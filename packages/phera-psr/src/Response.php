<?php

declare(strict_types=1);

namespace Phera\Psr;

use Phera\Http\Response as PheraResponse;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\StreamInterface;

/**
 * A PSR-7 response, convertible to and from `Phera\Http\Response`:
 *
 * ```php
 * return (new Response(201, ['Location' => '/users/7']))->toPheraResponse();
 * $psr = Response::fromPhera($client->get('/users/7'));
 * ```
 */
final class Response implements ResponseInterface
{
    use MessageTrait;

    private const PHRASES = [
        100 => 'Continue', 101 => 'Switching Protocols', 102 => 'Processing', 103 => 'Early Hints',
        200 => 'OK', 201 => 'Created', 202 => 'Accepted', 203 => 'Non-Authoritative Information',
        204 => 'No Content', 205 => 'Reset Content', 206 => 'Partial Content', 207 => 'Multi-Status',
        208 => 'Already Reported', 226 => 'IM Used',
        300 => 'Multiple Choices', 301 => 'Moved Permanently', 302 => 'Found', 303 => 'See Other',
        304 => 'Not Modified', 305 => 'Use Proxy', 307 => 'Temporary Redirect', 308 => 'Permanent Redirect',
        400 => 'Bad Request', 401 => 'Unauthorized', 402 => 'Payment Required', 403 => 'Forbidden',
        404 => 'Not Found', 405 => 'Method Not Allowed', 406 => 'Not Acceptable',
        407 => 'Proxy Authentication Required', 408 => 'Request Timeout', 409 => 'Conflict', 410 => 'Gone',
        411 => 'Length Required', 412 => 'Precondition Failed', 413 => 'Content Too Large',
        414 => 'URI Too Long', 415 => 'Unsupported Media Type', 416 => 'Range Not Satisfiable',
        417 => 'Expectation Failed', 418 => "I'm a teapot", 421 => 'Misdirected Request',
        422 => 'Unprocessable Content', 423 => 'Locked', 424 => 'Failed Dependency', 425 => 'Too Early',
        426 => 'Upgrade Required', 428 => 'Precondition Required', 429 => 'Too Many Requests',
        431 => 'Request Header Fields Too Large', 451 => 'Unavailable For Legal Reasons',
        500 => 'Internal Server Error', 501 => 'Not Implemented', 502 => 'Bad Gateway',
        503 => 'Service Unavailable', 504 => 'Gateway Timeout', 505 => 'HTTP Version Not Supported',
        506 => 'Variant Also Negotiates', 507 => 'Insufficient Storage', 508 => 'Loop Detected',
        510 => 'Not Extended', 511 => 'Network Authentication Required',
    ];

    private int $statusCode;

    private string $reasonPhrase;

    /** @param array<string, string|list<string>> $headers */
    public function __construct(
        int $status = 200,
        array $headers = [],
        StreamInterface|string|null $body = null,
        string $version = '1.1',
        string $reason = '',
    ) {
        self::assertStatus($status);
        $this->statusCode = $status;
        $this->reasonPhrase = $reason !== '' ? $reason : (self::PHRASES[$status] ?? '');
        $this->protocol = $version;
        $this->setHeaders($headers);
        if ($body !== null && $body !== '') {
            $this->stream = $body instanceof StreamInterface ? $body : Stream::create($body);
        }
    }

    /** The PSR-7 view of a `Phera\Http\Response` (from a handler or `Phera\Http\Client`). */
    public static function fromPhera(PheraResponse $response): self
    {
        return new self($response->status(), $response->headers(), $response->body());
    }

    /** What any PSR-7 response sends through `phera serve`. */
    public static function toPhera(ResponseInterface $response): PheraResponse
    {
        $body = $response->getBody();
        if ($body->isSeekable()) {
            $body->rewind();
        }
        return PheraResponse::make($body->getContents(), $response->getStatusCode(), $response->getHeaders());
    }

    public function toPheraResponse(): PheraResponse
    {
        return self::toPhera($this);
    }

    public function getStatusCode(): int
    {
        return $this->statusCode;
    }

    public function withStatus(int $code, string $reasonPhrase = ''): static
    {
        self::assertStatus($code);
        $copy = clone $this;
        $copy->statusCode = $code;
        $copy->reasonPhrase = $reasonPhrase !== '' ? $reasonPhrase : (self::PHRASES[$code] ?? '');
        return $copy;
    }

    public function getReasonPhrase(): string
    {
        return $this->reasonPhrase;
    }

    private static function assertStatus(int $status): void
    {
        if ($status < 100 || $status > 599) {
            throw new \InvalidArgumentException("Status $status is not between 100 and 599");
        }
    }
}
