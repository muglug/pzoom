<?php
$sum = 0;
foreach ([-6, 0, 2] as $i) {
    if ($i > 0) {
        $sum += $i;
    }
}
takesNonNegativeInt($sum);

/** @psalm-param int<0, max> $i */
function takesNonNegativeInt(int $i): void{
    return;
}
