<?php
$map = require '/Users/brownmatthew/git/psalm/dictionaries/PropertyMap.php';
ksort($map);
foreach ($map as &$props) { ksort($props); }
file_put_contents(
    '/Users/brownmatthew/git/pzoom/dictionaries/property_map.json',
    json_encode($map, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES) . "\n"
);
echo count($map), " classes\n";
