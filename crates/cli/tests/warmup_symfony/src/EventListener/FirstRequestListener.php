<?php

namespace App\EventListener;

use Symfony\Component\EventDispatcher\Attribute\AsEventListener;
use Symfony\Component\HttpFoundation\RequestStack;
use Symfony\Component\HttpKernel\Event\ResponseEvent;

/**
 * A listener that reads the request when it is built, as application code
 * sometimes does. Symfony builds a listener when its event is first
 * dispatched, which is inside a request; built before the snapshot, without
 * a request, it would remember "none" for every request.
 */
#[AsEventListener]
final class FirstRequestListener
{
    private readonly string $builtFor;

    public function __construct(RequestStack $requests)
    {
        $this->builtFor = $requests->getCurrentRequest()?->getPathInfo() ?? 'none';
    }

    public function __invoke(ResponseEvent $event): void
    {
        $event->getResponse()->headers->set('X-Listener-Built-For', $this->builtFor);
    }
}
