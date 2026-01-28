<?php
/**
 * @param  array{b:string} $a
 * @return null|string
 */
function fooFoo($a) {
    if ($a["b"]) {
        return $a["b"];
    }
}
