<?php

/**
 * @template T of int|string
 * @param T $p
 */
function foo($p): void {
    if (is_int($p)) {
        $q = $p - 1;
    }
}