<?php
// A small page the way an application builds one: three "queries" that each
// take 20 ms, and some work on the rows in between.
function query(string $table, int $rows): array {
    usleep(20_000);
    $result = [];
    for ($i = 1; $i <= $rows; $i++) {
        $result[] = ['id' => $i, 'name' => ucfirst($table) . " $i", 'score' => ($i * 37) % 101];
    }
    return $result;
}

$users = query('user', 50);
$posts = query('post', 200);
$tags = query('tag', 30);

usort($posts, fn(array $a, array $b) => $b['score'] <=> $a['score']);
$top = array_slice($posts, 0, 10);
$names = array_map(fn(array $user) => htmlspecialchars($user['name']), $users);

header('Content-Type: text/html; charset=utf-8');
echo "<h1>", count($users), " users, ", count($posts), " posts, ", count($tags), " tags</h1>\n<ol>\n";
foreach ($top as $post) {
    echo "  <li>", htmlspecialchars($post['name']), " (", $post['score'], ")</li>\n";
}
echo "</ol>\n<p>", implode(', ', array_slice($names, 0, 5)), "</p>\n";
