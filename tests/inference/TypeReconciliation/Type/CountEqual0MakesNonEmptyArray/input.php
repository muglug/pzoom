<?php
function a(array $a): void {
    if (count($a) === 0) {
        throw new \LogicException;
    }
    expectNonEmptyArray($a);
}
function b(array $a): void {
    if (count($a) !== 0) {
        expectNonEmptyArray($a);
    }
}
function c(array $a): void {
    if (count($a) === 0) {
        throw new \LogicException;
    } else {
        expectNonEmptyArray($a);
    }
}
/** @param non-empty-array $a */
function expectNonEmptyArray(array $a): array { return $a; }