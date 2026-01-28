<?php
class A {}
interface I {}
class AChild extends A implements I {}

function isAChild(A $value): ?AChild {
    if (!$value instanceof I) {
        return null;
    }

    if (!$value instanceof AChild) {
        return null;
    }

    return $value;
}