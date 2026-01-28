<?php
/** @param callable|false $x */
function example($x) : void {
    if (is_array($x)) {
        $c = count($x);
        if ($c !== 2) {}
    }
}
