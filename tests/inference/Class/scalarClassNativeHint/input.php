<?php
namespace NS;

abstract class Scalar { public int $v = 1; }
final class MyString extends Scalar {}

function f(Scalar $s): int {
    if ($s instanceof MyString) {
        return 1;
    }
    return $s->v;
}
