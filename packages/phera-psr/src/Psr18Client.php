<?php

declare(strict_types=1);

namespace Phera\Psr;

use Phera\Http\Client;
use Phera\Http\ClientException;
use Psr\Http\Client\ClientInterface;
use Psr\Http\Message\RequestInterface;
use Psr\Http\Message\ResponseInterface;

/**
 * `Phera\Http\Client` as a PSR-18 client:
 *
 * ```php
 * $client = new Psr18Client(new Client(['timeout' => 5.0]));
 * $response = $client->sendRequest((new HttpFactory())->createRequest('GET', 'https://example.com/'));
 * ```
 *
 * As PSR-18 asks, any HTTP status is a response, not an exception, and a
 * redirect is returned as it is unless the wrapped client follows them.
 */
final class Psr18Client implements ClientInterface
{
    private readonly Client $client;

    public function __construct(?Client $client = null)
    {
        $this->client = $client ?? new Client(['max_redirects' => 0]);
    }

    public function sendRequest(RequestInterface $request): ResponseInterface
    {
        $body = $request->getBody();
        if ($body->isSeekable()) {
            $body->rewind();
        }
        try {
            $response = $this->client->request($request->getMethod(), (string) $request->getUri(), [
                'headers' => $request->getHeaders(),
                'body' => $body->getContents(),
            ]);
        } catch (ClientException $exception) {
            throw new NetworkException($request, $exception->getMessage(), $exception);
        } catch (\ValueError $error) {
            throw new RequestException($request, $error->getMessage(), $error);
        }
        return Response::fromPhera($response);
    }
}
