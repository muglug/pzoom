<?php
interface A {}
interface I {}
class AChild implements I, A {}

function isAChild(A $value): ?AChild {
    if (!$value instanceof I) {
        return null;
    }

    if (!$value instanceof AChild) {
        return null;
    }

    return $value;
}