<?php

declare(strict_types=1);

namespace Phera\Psr;

use Psr\Http\Message\StreamInterface;

/**
 * A PSR-7 stream over a string, or over a PHP stream resource.
 *
 * Bodies are usually whole strings (a Phera request or response body), so
 * a string stream keeps the string itself and only a position: reading it
 * all back is free. `createFromResource()` and `createFromFile()` wrap a
 * real stream instead.
 */
final class Stream implements StreamInterface
{
    /** @var resource|null */
    private $resource = null;

    private string $buffer = '';

    private int $position = 0;

    private bool $detached = false;

    private function __construct() {}

    public static function create(string $content = ''): self
    {
        $stream = new self();
        $stream->buffer = $content;
        return $stream;
    }

    /** @param resource $resource */
    public static function createFromResource($resource): self
    {
        if (!is_resource($resource)) {
            throw new \InvalidArgumentException('A stream resource is required');
        }
        $stream = new self();
        $stream->resource = $resource;
        return $stream;
    }

    public static function createFromFile(string $filename, string $mode = 'r'): self
    {
        if ($mode === '' || strpbrk($mode[0], 'rwaxc') === false) {
            throw new \InvalidArgumentException("\"$mode\" is not a valid fopen() mode");
        }
        $resource = @fopen($filename, $mode);
        if ($resource === false) {
            throw new \RuntimeException("Cannot open \"$filename\": " . (error_get_last()['message'] ?? 'unknown error'));
        }
        return self::createFromResource($resource);
    }

    public function __toString(): string
    {
        if ($this->detached) {
            return '';
        }
        if ($this->resource === null) {
            $this->position = strlen($this->buffer);
            return $this->buffer;
        }
        if ($this->isSeekable()) {
            $this->rewind();
        }
        return $this->getContents();
    }

    public function close(): void
    {
        if ($this->resource !== null) {
            fclose($this->resource);
        }
        $this->detach();
    }

    public function detach()
    {
        $resource = $this->resource;
        $this->resource = null;
        $this->buffer = '';
        $this->position = 0;
        $this->detached = true;
        return $resource;
    }

    public function getSize(): ?int
    {
        if ($this->detached) {
            return null;
        }
        if ($this->resource === null) {
            return strlen($this->buffer);
        }
        return fstat($this->resource)['size'] ?? null;
    }

    public function tell(): int
    {
        $this->assertAttached();
        if ($this->resource === null) {
            return $this->position;
        }
        $position = ftell($this->resource);
        if ($position === false) {
            throw new \RuntimeException('Cannot tell the stream position');
        }
        return $position;
    }

    public function eof(): bool
    {
        if ($this->detached) {
            return true;
        }
        return $this->resource === null ? $this->position >= strlen($this->buffer) : feof($this->resource);
    }

    public function isSeekable(): bool
    {
        if ($this->detached) {
            return false;
        }
        return $this->resource === null || (bool) $this->getMetadata('seekable');
    }

    public function seek(int $offset, int $whence = SEEK_SET): void
    {
        $this->assertAttached();
        if ($this->resource !== null) {
            if (!$this->isSeekable() || fseek($this->resource, $offset, $whence) === -1) {
                throw new \RuntimeException('Cannot seek the stream');
            }
            return;
        }
        $target = match ($whence) {
            SEEK_SET => $offset,
            SEEK_CUR => $this->position + $offset,
            SEEK_END => strlen($this->buffer) + $offset,
            default => throw new \RuntimeException("Invalid whence $whence"),
        };
        if ($target < 0) {
            throw new \RuntimeException("Cannot seek to position $target");
        }
        $this->position = $target;
    }

    public function rewind(): void
    {
        $this->seek(0);
    }

    public function isWritable(): bool
    {
        if ($this->detached) {
            return false;
        }
        if ($this->resource === null) {
            return true;
        }
        $mode = (string) $this->getMetadata('mode');
        return strpbrk($mode, 'waxc+') !== false;
    }

    public function write(string $string): int
    {
        $this->assertAttached();
        if ($this->resource !== null) {
            $written = $this->isWritable() ? fwrite($this->resource, $string) : false;
            if ($written === false) {
                throw new \RuntimeException('Cannot write to the stream');
            }
            return $written;
        }
        $length = strlen($string);
        if ($this->position >= strlen($this->buffer)) {
            $this->buffer = str_pad($this->buffer, $this->position, "\0") . $string;
        } else {
            $this->buffer = substr_replace($this->buffer, $string, $this->position, $length);
        }
        $this->position += $length;
        return $length;
    }

    public function isReadable(): bool
    {
        if ($this->detached) {
            return false;
        }
        if ($this->resource === null) {
            return true;
        }
        $mode = (string) $this->getMetadata('mode');
        return str_contains($mode, 'r') || str_contains($mode, '+');
    }

    public function read(int $length): string
    {
        $this->assertAttached();
        if ($length < 0) {
            throw new \RuntimeException('The length must not be negative');
        }
        if ($this->resource !== null) {
            $data = $this->isReadable() ? fread($this->resource, max(1, $length)) : false;
            if ($data === false) {
                throw new \RuntimeException('Cannot read from the stream');
            }
            return $length === 0 ? '' : $data;
        }
        $data = (string) substr($this->buffer, $this->position, $length);
        $this->position += strlen($data);
        return $data;
    }

    public function getContents(): string
    {
        $this->assertAttached();
        if ($this->resource !== null) {
            $contents = stream_get_contents($this->resource);
            if ($contents === false) {
                throw new \RuntimeException('Cannot read from the stream');
            }
            return $contents;
        }
        $contents = $this->position === 0 ? $this->buffer : (string) substr($this->buffer, $this->position);
        $this->position = max($this->position, strlen($this->buffer));
        return $contents;
    }

    public function getMetadata(?string $key = null)
    {
        if ($this->detached) {
            return $key === null ? [] : null;
        }
        $metadata = $this->resource === null
            ? ['timed_out' => false, 'blocked' => true, 'eof' => $this->eof(), 'wrapper_type' => 'PHP',
               'stream_type' => 'MEMORY', 'mode' => 'r+b', 'unread_bytes' => 0, 'seekable' => true, 'uri' => 'php://memory']
            : stream_get_meta_data($this->resource);
        return $key === null ? $metadata : ($metadata[$key] ?? null);
    }

    private function assertAttached(): void
    {
        if ($this->detached) {
            throw new \RuntimeException('The stream is detached');
        }
    }
}
