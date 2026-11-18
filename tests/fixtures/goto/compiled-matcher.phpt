--TEST--
A compiled matcher jumps among switch labels inside a loop
--FILE--
<?php
function matchRoute(string $path) {
    foreach (explode('/', $path) as $segment) {
        switch ($segment) {
            case 'article':
                goto article;
            case 'user':
                goto user;
            case 'admin':
                goto admin;
            default:
                goto nextSegment;
            article:
                $route = 'article';
                goto matched;
            user:
                $route = 'user';
                goto matched;
            admin:
                $route = 'admin';
                goto matched;
            matched:
                echo $route, "\n";
            nextSegment:
        }
    }
}
matchRoute('ignored/article/user/admin');
--EXPECT--
article
user
admin
