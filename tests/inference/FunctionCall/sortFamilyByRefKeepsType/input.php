<?php
/** @param array<int, DateTime> $m */
function f(array $m): void {
    krsort($m, SORT_NUMERIC);
    foreach ($m as $pos => $loc) {
        echo (string) $pos;
        echo $loc->format('Y');
    }
}
/** @param array<string, int> $n */
function g(array $n): int {
    asort($n);
    $total = 0;
    foreach ($n as $v) {
        $total += $v;
    }
    return $total;
}
