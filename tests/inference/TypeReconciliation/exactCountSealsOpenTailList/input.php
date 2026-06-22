<?php
/** @param list{string, string} $p */
function takesPair(array $p): void {}

/**
 * After `count($a) === 2`, an open-tail list `list{string, string, ...<string>}`
 * is sealed to exactly `list{string, string}`, so it is assignable to a
 * two-element pair parameter (no InvalidArgument).
 *
 * @param list{0: string, 1: string, ...<string>} $a
 */
function f(array $a): void {
    if (count($a) !== 2) {
        return;
    }
    takesPair($a);
}
