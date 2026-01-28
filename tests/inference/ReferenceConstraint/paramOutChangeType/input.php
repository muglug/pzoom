<?php
/**
 * @param-out int $s
 */
function addFoo(?string &$s) : void {
    if ($s === null) {
        $s = 5;
        return;
    }
    $s = 4;
}

addFoo($a);
