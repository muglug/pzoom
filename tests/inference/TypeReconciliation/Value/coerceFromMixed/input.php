<?php
function type(int $b): void {}

/**
 * @param mixed $a
 */
function foo($a) : void {
    if ($a === 1 || $a === 2) {
        type($a);
    }

    if (in_array($a, [1, 2], true)) {
        type($a);
    }
}