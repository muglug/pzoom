<?php
/**
 * @param-out bool $s
 */
function foo(?bool &$s) : void {
    $s = true;
}

$b = false;
foo($b);
