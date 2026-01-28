<?php
/**
 * @param-out int $s
 */
function addFoo(bool $five = true, ?string &$s = null) : void {
    if ($five) {
        $s = 5;
        return;
    }
    $s = 4;
}

addFoo(s: $a);
