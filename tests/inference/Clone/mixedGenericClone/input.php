<?php
/**
 * @template T
 * @param T $a
 */
function foo($a): void {
    clone $a;
}
