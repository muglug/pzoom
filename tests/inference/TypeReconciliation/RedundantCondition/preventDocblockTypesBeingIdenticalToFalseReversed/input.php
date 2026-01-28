<?php
class A {}

/**
 * @param  A $a
 */
function foo($a, $b) : void {
    if (false === $a) {}
}
