<?php

namespace App\Controller;

use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Request;
use Symfony\Component\Routing\Attribute\Route;

final class VisitController
{
    /** Remembers a name in the session and reports what the session held before. */
    #[Route('/visit', methods: ['GET'])]
    public function visit(Request $request): JsonResponse
    {
        $session = $request->getSession();
        $before = $session->get('name');
        if (null !== $request->query->get('name')) {
            $session->set('name', $request->query->get('name'));
        }

        return new JsonResponse(['before' => $before]);
    }
}
