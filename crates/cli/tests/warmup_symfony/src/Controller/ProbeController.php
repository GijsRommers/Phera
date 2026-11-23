<?php

namespace App\Controller;

use App\Entity\Product;
use Doctrine\ORM\EntityManagerInterface;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Request;
use Symfony\Component\HttpFoundation\RequestStack;
use Symfony\Component\Routing\Attribute\Route;
use Symfony\Component\Routing\Generator\UrlGeneratorInterface;

final class ProbeController
{
    public function __construct(
        private readonly EntityManagerInterface $entities,
        private readonly RequestStack $requests,
        private readonly UrlGeneratorInterface $urls,
    ) {
    }

    #[Route('/api/ping', methods: ['GET'])]
    public function ping(Request $request): JsonResponse
    {
        return new JsonResponse(['pong' => true, 'name' => $request->query->get('name', 'world')]);
    }

    /** Doctrine, with a query sent to the database. */
    #[Route('/api/products', methods: ['GET'])]
    public function products(): JsonResponse
    {
        $products = $this->entities->getRepository(Product::class)->findBy([], ['id' => 'ASC']);

        return new JsonResponse(array_map(fn (Product $product) => [$product->id, $product->name, $product->price], $products));
    }

    /**
     * Everything a request could inherit from the worker's boot. Values that
     * differ between any two requests are reported as facts about them, so
     * the whole response can be compared with PHP's.
     */
    #[Route('/api/state', methods: ['GET'])]
    public function state(Request $request): JsonResponse
    {
        $now = microtime(true);

        return new JsonResponse([
            'request' => [
                'is_the_main_one' => $this->requests->getMainRequest() === $request,
                'host' => $request->getHost(),
                'url' => $this->urls->generate('app_probe_ping', [], UrlGeneratorInterface::ABSOLUTE_URL),
                'header' => $request->headers->get('X-Probe'),
                'cookies' => $request->cookies->all(),
            ],
            'session_started' => $request->hasSession() && $request->getSession()->isStarted(),
            'time' => [
                'request_time_is_this_request' => $now - $request->server->get('REQUEST_TIME_FLOAT') < 30,
                'now_is_this_request' => abs((new \DateTimeImmutable())->getTimestamp() - $now) < 30,
            ],
            'entities_managed' => $this->entities->getUnitOfWork()->size(),
            'transaction_open' => $this->entities->getConnection()->isTransactionActive(),
            'output_buffers' => ob_get_level(),
        ]);
    }

    /** Values that must be drawn in the request, never in the worker's boot. */
    #[Route('/api/random', methods: ['GET'])]
    public function random(): JsonResponse
    {
        return new JsonResponse([random_int(0, PHP_INT_MAX), mt_rand(), bin2hex(random_bytes(8)), uniqid('', true)]);
    }

    /**
     * What the worker's boot left in the process, as this request found it.
     * Only Phera has such a boot: this answer is not compared with PHP's.
     */
    #[Route('/api/warmed', methods: ['GET'])]
    public function warmed(): JsonResponse
    {
        return new JsonResponse([
            'metadata_loaded' => array_keys($this->entities->getMetadataFactory()->getLoadedMetadata()),
            'connected' => $this->entities->getConnection()->isConnected(),
        ]);
    }
}
