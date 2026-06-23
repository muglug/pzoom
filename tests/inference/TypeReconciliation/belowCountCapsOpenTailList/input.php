<?php
/** @param list{0?: string, 1?: string} $p */
function takesUpToTwo(array $p): void {}

/**
 * After `count($a) > 2` is excluded, an open-tail list holds at most two
 * elements, so the fallback tail is dropped (`list{0: string, 1?: string}`) and
 * it is assignable to a parameter that accepts up to two elements.
 *
 * @param list{0: string, 1: string, ...<string>} $a
 */
function f(array $a): void {
    if (count($a) > 2) {
        return;
    }
    takesUpToTwo($a);
}
