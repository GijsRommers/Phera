<?php

declare(strict_types=1);

// PSR-3, PSR-7 and PSR-17 adapters, without a network: `phera tests/unit.php`.

require __DIR__ . '/bootstrap.php';

use Phera\Log\Logger;
use Phera\Psr\HttpFactory;
use Phera\Psr\Pipeline;
use Phera\Psr\Psr3Logger;
use Phera\Psr\Request;
use Phera\Psr\Response;
use Phera\Psr\ServerRequest;
use Phera\Psr\Stream;
use Phera\Psr\Uri;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Server\MiddlewareInterface;
use Psr\Http\Server\RequestHandlerInterface;
use Psr\Log\InvalidArgumentException as PsrInvalidArgument;
use Psr\Log\LoggerInterface;
use Psr\Log\LogLevel;

// PSR-3
$native = Logger::memory('psr', 'debug');
$logger = new Psr3Logger($native);
Check::true($logger instanceof LoggerInterface, 'Psr3Logger is a LoggerInterface');
$levels = [LogLevel::EMERGENCY, LogLevel::ALERT, LogLevel::CRITICAL, LogLevel::ERROR,
    LogLevel::WARNING, LogLevel::NOTICE, LogLevel::INFO, LogLevel::DEBUG];
foreach ($levels as $level) {
    $logger->$level("via $level {n}", ['n' => 1]);
    $logger->log($level, "log $level");
}
$records = $native->records();
Check::same(16, count($records), 'all eight levels, both ways');
Check::same(['emergency', 'via emergency 1'], [$records[0]['level'], $records[0]['message']], 'PSR-3 interpolation');
Check::same('log debug', $records[15]['message'], 'log() with a level');
$stringable = new class { public function __toString(): string { return 'from Stringable'; } };
$logger->info($stringable);
Check::same('from Stringable', $native->records()[16]['message'], 'Stringable message');
Check::throws(PsrInvalidArgument::class, fn () => $logger->log('loud', 'x'), 'unknown level');
Check::throws(PsrInvalidArgument::class, fn () => $logger->log(3, 'x'), 'non-string level');
$logger->withContext(['request' => 'r1'])->warning('child');
Check::same(['request' => 'r1'], $native->records()[17]['context'], 'withContext');
$quiet = new Psr3Logger(Logger::memory('quiet', 'error'));
$quiet->info('dropped');
$quiet->error('kept');
Check::same(['kept'], array_column($quiet->logger()->records(), 'message'), 'level filter');

// PSR-7 Uri
$uri = new Uri('HTTPS://user:p%40ss@Example.COM:443/a b/c?x=1&y=ä#frag');
Check::same('https', $uri->getScheme(), 'scheme lower-cased');
Check::same('example.com', $uri->getHost(), 'host lower-cased');
Check::same(null, $uri->getPort(), 'default port dropped');
Check::same('user:p%40ss@example.com', $uri->getAuthority(), 'authority');
Check::same('/a%20b/c', $uri->getPath(), 'path encoded');
Check::same('x=1&y=%C3%A4', $uri->getQuery(), 'query encoded');
Check::same('https://user:p%40ss@example.com/a%20b/c?x=1&y=%C3%A4#frag', (string) $uri, 'uri string');
$changed = $uri->withScheme('http')->withPort(8080)->withPath('rel')->withQuery('?q=1')->withFragment('');
Check::same('http://user:p%40ss@example.com:8080/rel?q=1', (string) $changed, 'with* copies');
Check::same('https://user:p%40ss@example.com/a%20b/c?x=1&y=%C3%A4#frag', (string) $uri, 'original unchanged');
Check::same('/only/path', (string) new Uri('/only/path'), 'relative reference');
Check::throws(\InvalidArgumentException::class, fn () => (new Uri())->withPort(70000), 'port range');

// PSR-7 Stream
$stream = Stream::create('hello');
Check::same([5, 'hel', 3, 'lo', true], [$stream->getSize(), $stream->read(3), $stream->tell(), $stream->getContents(), $stream->eof()], 'string stream reads');
$stream->seek(0, SEEK_END);
$stream->write(' world');
Check::same('hello world', (string) $stream, 'string stream writes');
$stream->seek(1);
$stream->write('E');
Check::same('hEllo world', (string) $stream, 'overwrite in place');
$file = tempnam(sys_get_temp_dir(), 'phera-psr');
file_put_contents($file, 'file body');
$fromFile = (new HttpFactory())->createStreamFromFile($file);
Check::same(['file body', 9, false], [(string) $fromFile, $fromFile->getSize(), $fromFile->isWritable()], 'file stream');
$fromFile->close();
unlink($file);
Check::throws(\RuntimeException::class, fn () => Stream::createFromFile('/no/such/file'), 'missing file');
Check::throws(\InvalidArgumentException::class, fn () => Stream::createFromFile(__FILE__, 'z'), 'bad mode');
$detached = Stream::create('x');
$detached->detach();
Check::same(['', null, false], [(string) $detached, $detached->getSize(), $detached->isReadable()], 'detached stream');

// PSR-7 Request and Response
$request = new Request('POST', 'https://api.example.com:8443/users?x=1', ['X-Tag' => ['a', 'b'], 'content-type' => 'text/plain'], 'body');
Check::same(['Host', 'X-Tag', 'content-type'], array_keys($request->getHeaders()), 'Host first');
Check::same('api.example.com:8443', $request->getHeaderLine('host'), 'Host from URI');
Check::same('a, b', $request->getHeaderLine('x-tag'), 'header line');
Check::same(['text/plain'], $request->getHeader('Content-Type'), 'case-insensitive header');
Check::same('/users?x=1', $request->getRequestTarget(), 'request target');
Check::same('body', (string) $request->getBody(), 'request body');
$added = $request->withAddedHeader('x-tag', 'c')->withoutHeader('CONTENT-TYPE')->withMethod('PUT');
Check::same([['a', 'b', 'c'], false, 'PUT'], [$added->getHeader('X-Tag'), $added->hasHeader('content-type'), $added->getMethod()], 'with* headers');
Check::same(['a', 'b'], $request->getHeader('X-Tag'), 'request unchanged');
$moved = $request->withUri(new Uri('http://other.test/'));
Check::same('other.test', $moved->getHeaderLine('Host'), 'withUri updates Host');
Check::same('api.example.com:8443', $request->withUri(new Uri('http://other.test/'), true)->getHeaderLine('Host'), 'preserveHost');
Check::throws(\InvalidArgumentException::class, fn () => $request->withHeader("Bad\nName", 'x'), 'bad header name');
Check::throws(\InvalidArgumentException::class, fn () => $request->withHeader('X', "a\r\nb"), 'header injection');

$response = new Response(201, ['Location' => '/users/7', 'Set-Cookie' => ['a=1', 'b=2']], '{"id":7}');
Check::same([201, 'Created', '1.1'], [$response->getStatusCode(), $response->getReasonPhrase(), $response->getProtocolVersion()], 'response status');
Check::same([418, 'Tea'], [$response->withStatus(418, 'Tea')->getStatusCode(), $response->withStatus(418, 'Tea')->getReasonPhrase()], 'withStatus');
Check::throws(\InvalidArgumentException::class, fn () => $response->withStatus(99), 'status range');
$phera = $response->toPheraResponse();
Check::true($phera instanceof \Phera\Http\Response, 'toPheraResponse');
Check::same([201, '/users/7', ['a=1', 'b=2'], '{"id":7}', null], [$phera->status(), $phera->header('location'), $phera->headers()['set-cookie'], $phera->body(), $phera->header('content-type')], 'Phera response keeps exactly the PSR headers');
$back = Response::fromPhera(\Phera\Http\Response::json(['ok' => true], 202));
Check::same([202, 'application/json', '{"ok":true}'], [$back->getStatusCode(), $back->getHeaderLine('Content-Type'), (string) $back->getBody()], 'fromPhera');

// PSR-17
$factory = new HttpFactory();
Check::same('GET', $factory->createRequest('GET', 'http://x.test/')->getMethod(), 'createRequest');
Check::same([404, 'Not Found'], [$factory->createResponse(404)->getStatusCode(), $factory->createResponse(404)->getReasonPhrase()], 'createResponse');
$server = $factory->createServerRequest('GET', 'http://x.test/p?q=1', ['REMOTE_ADDR' => '127.0.0.1']);
Check::same([['q' => '1'], ['REMOTE_ADDR' => '127.0.0.1']], [$server->getQueryParams(), $server->getServerParams()], 'createServerRequest');
Check::same('abc', (string) $factory->createStream('abc'), 'createStream');
$resource = fopen('php://memory', 'r+');
fwrite($resource, 'res');
rewind($resource);
Check::same('res', (string) $factory->createStreamFromResource($resource), 'createStreamFromResource');
Check::same('https://h.test/', (string) $factory->createUri('https://h.test/'), 'createUri');

// PSR-7 ServerRequest attributes and parsed body
$withAttributes = $server->withAttribute('id', 7)->withParsedBody(['a' => 1])->withCookieParams(['s' => 'x']);
Check::same([7, 'd', ['a' => 1], ['s' => 'x']], [$withAttributes->getAttribute('id'), $withAttributes->getAttribute('nope', 'd'), $withAttributes->getParsedBody(), $withAttributes->getCookieParams()], 'server request state');
Check::same([], $withAttributes->withoutAttribute('id')->getAttributes(), 'withoutAttribute');
Check::same([], $server->getAttributes(), 'original server request unchanged');

// PSR-15 pipeline order, without a server
$trace = new ArrayObject();
$middleware = fn (string $name) => new class ($name, $trace) implements MiddlewareInterface {
    public function __construct(private string $name, private ArrayObject $trace) {}
    public function process(ServerRequestInterface $request, RequestHandlerInterface $handler): ResponseInterface
    {
        $this->trace[] = "in {$this->name}";
        $response = $handler->handle($request->withAttribute($this->name, true));
        $this->trace[] = "out {$this->name}";
        return $response->withAddedHeader('X-Via', $this->name);
    }
};
$handler = new class ($trace) implements RequestHandlerInterface {
    public function __construct(private ArrayObject $trace) {}
    public function handle(ServerRequestInterface $request): ResponseInterface
    {
        $this->trace[] = 'handler ' . implode(',', array_keys($request->getAttributes()));
        return new Response(200);
    }
};
$result = (new Pipeline($handler, [$middleware('a'), $middleware('b')]))->handle($server);
Check::same(['in a', 'in b', 'handler a,b', 'out b', 'out a'], $trace->getArrayCopy(), 'middleware order');
Check::same(['b', 'a'], $result->getHeader('X-Via'), 'responses unwind');

Check::report('phera-psr unit');
