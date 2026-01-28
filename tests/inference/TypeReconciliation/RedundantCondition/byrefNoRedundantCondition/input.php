<?php
/**
 * @param int $min ref
 * @param int $other
 */
function testmin(&$min, int $other): void {
    if (is_null($min)) {
        $min = 3;
    } elseif (!is_int($min)) {
        $min = 5;
    } elseif ($min < $other) {
        $min = $other;
    }
}