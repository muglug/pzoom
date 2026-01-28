<?php
function foo(?stdClass $a, ?stdClass $b, ?stdClass $c): stdClass {
    if ($a) {
        // do nothing
    } elseif ($b) {
        // do nothing here
    } elseif ($c) {
        // do nothing here
    } else {
        return new stdClass;
    }

    if (!$a && !$b) {
        return $c;
    }
    if (!$a) return $b;
    return $a;
}