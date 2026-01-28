<?php
$a = [
    "b" => [],
];

$a["b"]["c"] = 0;

foreach ([1, 2, 3] as $i) {
    /**
     * @psalm-suppress InvalidArrayOffset
     * @psalm-suppress MixedOperand
     * @psalm-suppress PossiblyUndefinedArrayOffset
     * @psalm-suppress MixedAssignment
     */
    $a["b"]["d"] += $a["b"][$i];
}
