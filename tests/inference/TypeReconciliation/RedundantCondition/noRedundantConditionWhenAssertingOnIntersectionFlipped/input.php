<?php
class A {}
interface I {}
class AChild extends A implements I {}

/** @param I&A $value */
function isAChild(I $value): ?AChild {
    if (!$value instanceof AChild) {
        return null;
    }

    return $value;
}