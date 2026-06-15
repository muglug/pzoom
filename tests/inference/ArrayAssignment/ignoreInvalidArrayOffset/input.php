<?php
$a = [
    "b" => [],
];

$a["b"]["c"] = 0;

foreach ([1, 2, 3] as $i) {
    /**
     * @psalm-suppress InvalidArrayOffset
     * @psalm-suppress PossiblyUndefinedArrayOffset
     */
    $a["b"]["d"] += $a["b"][$i];
}
