<?php
class B {}
class C {}

function foo(?B $b, ?C $c): B|C {
    if (!$b && !$c) {
        throw new Exception("bad");
    }

    if ($b && $c) {
        return rand(0, 1) ? $b : $c;
    }

    if ($b) {
        return $b;
    }

    return $c;
}
