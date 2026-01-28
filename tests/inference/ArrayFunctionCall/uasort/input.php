<?php
function foo (int $a, int $b): int {
    return $a > $b ? 1 : -1;
}
$manifest = ["a" => 1, "b" => 2];
uasort(
    $manifest,
    "foo"
);
$emptyManifest = [];
uasort(
    $emptyManifest,
    "foo"
);
                    
