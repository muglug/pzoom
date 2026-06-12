<?php
/**
 * @return stdClass
 */
function foo() : ?stdClass {
    return rand(0, 1) ? new stdClass : null;
}

$f = foo();
if ($f) {}
