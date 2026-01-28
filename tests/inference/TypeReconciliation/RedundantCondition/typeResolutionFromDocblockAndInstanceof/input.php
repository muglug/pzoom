<?php
class A { }

/**
 * @param  A $a
 * @return void
 * @psalm-suppress RedundantConditionGivenDocblockType
 */
function fooFoo($a) {
    if ($a instanceof A) {
        if ($a instanceof A) {
        }
    }
}
