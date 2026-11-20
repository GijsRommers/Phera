<?php
foreach (array_slice($argv, 1) as $path) {
    $fromPath = getimagesize($path, $extras);
    $fromMemory = getimagesizefromstring(file_get_contents($path), $memoryExtras);
    echo basename($path), '=', json_encode([$fromPath, $fromMemory, exif_imagetype($path),
        array_map('base64_encode', $extras), array_map('base64_encode', $memoryExtras)]), "\n";
}
for ($type = -2; $type <= IMAGETYPE_SVG; $type++) {
    echo $type, '=', json_encode([image_type_to_extension($type), image_type_to_extension($type, false),
        image_type_to_mime_type($type)]), "\n";
}
