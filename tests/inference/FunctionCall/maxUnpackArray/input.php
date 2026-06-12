<?php
$files = [
    __FILE__,
    __FILE__,
    __FILE__,
    __FILE__,
];

$a = array_map("filemtime", $files);
$b = array_map(
    function (string $file): int {
        return filemtime($file);
    },
    $files,
);
$A = max(filemtime(__FILE__), ...$a);
$B = max(filemtime(__FILE__), ...$b);

echo date("c", $A), "\n", date("c", $B);
