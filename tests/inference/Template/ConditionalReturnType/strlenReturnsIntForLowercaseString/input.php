<?php
/**
 * @psalm-return (
 *     $string is non-empty-string
 *     ? positive-int
 *     : int
 * )
 */
function strlen2(string $string) : int { return 1;}

function test(string $s): void {
    if (strlen2(strtolower($s))) {
        echo 1;
    }
}