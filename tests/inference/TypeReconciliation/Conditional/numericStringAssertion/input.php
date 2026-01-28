<?php
/**
 * @param mixed $a
 */
function foo($a, string $b) : void {
    if (is_numeric($b) && $a === $b) {
        echo $a;
    }
}