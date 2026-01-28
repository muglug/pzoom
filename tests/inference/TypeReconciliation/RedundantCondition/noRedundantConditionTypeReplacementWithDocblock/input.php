<?php
class A {}

/**
 * @return A
 */
function getA() {
    return new A();
}

$maybe_a = rand(0, 1) ? new A : null;

if ($maybe_a === null) {
    $maybe_a = getA();
}

if ($maybe_a === null) {}