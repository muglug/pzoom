<?php
class A {}
class B {}

/** @param A|B $a */
function foo($a) : void {
    if (!is_object($a)) {
        return;
    }

    if ($a instanceof A) {

    } elseif ($a instanceof B) {

    } else {
        throw new \Exception("bad");
    }
}