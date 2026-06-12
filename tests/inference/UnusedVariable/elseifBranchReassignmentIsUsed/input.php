<?php
function g2(int $i): int
{
    if ($i > 5) {
        $i = 7;
    } elseif ($i < -5) {
        $i = 0;
    }
    return $i;
}
